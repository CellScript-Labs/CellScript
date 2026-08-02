# CellScript Registry API

Typed production API for the public CellScript artifact Registry. The same
application runs as a Cloudflare Worker or through the bundled Node HTTP
adapter.

- `https://api.registry.cellscript.dev` is the authenticated write and dynamic
  read boundary.
- `https://registry.cellscript.dev` serves immutable bundles and static release
  JSON independently from the write database.

Postgres is authoritative for publisher capabilities, namespace ownership,
artifact releases, orthogonal release states, evidence, jobs, idempotency, and
audit events. R2 or the filesystem adapter stores immutable content and static
read objects.

## Artifact Contract

The API has one public resource family: `/v1/artifacts`. Every release declares
an artifact descriptor:

```ts
{
  kind: "source_library" | "profile_library" | "runtime_verifier" |
        "deployable_contract" | "reproducible_binary" | "template";
  profile: "cellscript_source" | "ckb_executable" |
           "reproducible_build" | "copy_material";
  consumption_mode: "dependency" | "deployment" | "tcb" | "copy";
  language: "cellscript" | "rust" | "c" | "javascript" |
            "other" | "unspecified";
}
```

Profile/kind/language/consumption combinations are closed and validated. The
generic profiles additionally carry a closed
`cellscript-registry-profile-contract-v1` object. Admission, the publisher CLI,
and the isolated verifier independently canonicalize it, bind its hash, reject
unknown fields, and verify the typed build/security/CKB/verifier/reproduction
or copy fields against immutable object hashes. The independent verifier then
applies a profile-specific object contract:

- `cellscript_source`: compile the canonical CellScript snapshot;
- `ckb_executable`: hash-bind source, executable, ABI, and an optional
  reproducible build recipe;
- `reproducible_build`: hash-bind source, executable, and build recipe, then
  require external reproducibility evidence;
- `copy_material`: hash-bind a `cellscript-template-file-map-v1` source and
  never treat it as a dependency.

Release state is split across:

```text
verification_status = pending | hash_bound | verified | evidence_required | rejected
deployment_status   = not_applicable | undeployed | deployed | chain_verified
availability_status = active | deprecated | yanked | quarantined
```

Publisher input can create only the initial states. Verification and deployment
states are derived from accepted evidence. Availability is the operator safety
axis and does not rewrite identity or evidence.

For a reproducible profile, `verified_build` with level `evidence_required` is
only the hash-bound predecessor. An admin promotion to `reproduced_build`
requires two to sixteen P-256-signed `cellscript-reproduction-report-v2`
reports. Each builder ID, public key, and trust domain must be distinct; every
builder must match `REGISTRY_REPRODUCER_POLICY_JSON`; and the reports must span
the policy's minimum number of trust domains. Reports bind the signed
environment, source hash, build-recipe hash, artifact hash, build-log hash,
timestamp, and predecessor evidence. Deployment admission rejects a
reproducible artifact until this transition succeeds. Accepted evidence also
stores the canonical policy SHA-256 and the minimum trust-domain threshold used
at acceptance time.

## Endpoints

```text
GET  /health
GET  /ready
GET  /artifacts/:namespace/:name/releases/:release.json
GET  /v1/artifacts
GET  /v1/artifacts/:namespace/:name
GET  /v1/artifacts/:namespace/:name/releases/:release/evidence
GET  /v1/artifacts/:namespace/:name/releases/:release/commitment
POST /v1/artifacts/:namespace/:name/releases
POST /v1/artifacts/:namespace/:name/releases/:release/deployments
POST /v1/artifacts/:namespace/:name/releases/:release/availability

POST /v1/capabilities
POST /v1/capabilities/:key_id/revoke
POST /v1/namespaces/claim

GET  /v1/admin/audit-events
GET  /v1/admin/verification-queue
POST /v1/admin/verification-jobs/:job_id/retry
POST /v1/admin/reserved-namespaces
POST /v1/admin/namespaces/:namespace/status
POST /v1/admin/artifacts/:namespace/:name/releases/:release/availability
POST /v1/admin/artifacts/:namespace/:name/releases/:release/promote
```

List filters are `q`, `namespace`, `kind`, `verification`, `deployment`,
`availability`, `limit`, and `offset`. Quarantined releases are absent from
public detail and evidence reads.

## Publisher Authorisation

Wallet-rooted capability authorisation supports:

- JoyID signatures under `principal_type = joyid_ckb`;
- recoverable CKB secp256k1 message signatures under
  `principal_type = ckb_secp256k1`.

The signature public key is bound to `principal_id`; a display address is not
an ACL key. The capability is P-256, scoped to `publish:namespace/name` or
`publish:namespace/*`, expiring, revocable, and stored separately from the
wallet root. Namespace ownership must match the capability principal.

```bash
cellc auth capability create --principal-type <principal_type> --principal-id <principal_id> --scope publish:ns/name --expires 90d --json > capability-payload.json
# Sign the canonical payload in a supported CKB wallet.
cellc auth capability submit --payload capability-payload.json --wallet-signature wallet-signature.json
cellc auth namespace claim --namespace ns --payload capability-payload.json --wallet-signature wallet-signature.json
```

Capability registration does not silently claim a namespace. Publish remains
blocked until the claim is active. Signed nonces are one-use; publish requests
also use an `Idempotency-Key` so exact retries replay safely and conflicting
content fails.

The browser wallet directory lists Neuron, JoyID, imToken, CKBull, SafePal,
Ledger, imKey, OneKey, UTXO Global, Rei Wallet, Gate, and QuantumPurse. Runtime
connectivity is determined by CCC discovery. Directory entries without a live
connector use the external signed-payload handoff and never bypass backend
signature verification. The service accepts no testnet authorisation or
deployment mode.

## Release Admission

Daily publish signs canonical JSON for:

```text
cellscript-registry-publish-v1 / publish
```

Admission requires:

- an active, unexpired, unrevoked capability with matching scope;
- an active namespace owned by the same principal;
- matching route, signed payload, artifact descriptor, coordinate, release,
  source hash, manifest hash, and single-release nested entry;
- a valid capability signature and unused nonce;
- a new release coordinate;
- a non-empty immutable snapshot/bundle no larger than 5 MiB;
- successful immutable-bundle and initial static-object writes.

Generic artifact profile contracts are closed and hash-bound. In particular,
an `audited` security declaration requires an immutable `audit_report` bundle
object bound by `security.audit_report_hash`; the isolated verifier recomputes
that hash before it emits evidence.

The database transaction stores the release, job, capability use, audit event,
nonce, and completed idempotency record. The verifier job is created in the
same transaction. An admission response does not claim verification.

CellScript packages publish with `cellc publish`; profile libraries add
`--artifact-kind profile_library`. Other artifacts publish with:

```bash
cellc publish --artifact-manifest Artifact.toml --dry-run
cellc publish --artifact-manifest Artifact.toml
```

`CELLSCRIPT_REGISTRY_API_URL` overrides the API base URL.
`CELLSCRIPT_CAPABILITY_PRIVATE_KEY_PKCS8_B64` supplies the delegated key in CI.
`CELLSCRIPT_REGISTRY_IDEMPOTENCY_KEY` pins the exact retry key.

## Mainnet Deployment Evidence

Executable publication begins at `deployment_status = undeployed`. A publisher
records a deployment by signing canonical JSON for:

```text
cellscript-registry-deployment / record_deployment
```

The request must identify `network = mainnet`, the published executable hash,
equal Cell data hash, code hash, hash type, dep type, and OutPoint. Prior
verified-build evidence is mandatory.

The API calls CKB mainnet `get_live_cell(out_point, true, false)` and fails
closed unless the Cell is live and its data hash equals the published
executable. For `hash_type = type`, it serializes the returned Type Script with
Molecule and verifies its CKB Script hash against `code_hash`. Data-hash modes
require `code_hash` to equal the data hash. Success appends hash-addressed
evidence and sets only `deployment_status = chain_verified`.

`CKB_MAINNET_RPC_URL` may override the default official mainnet RPC endpoint.
No testnet network value is accepted.

## Registry Chain Commitments

After deployment evidence exists, the public commitment endpoint returns the
canonical payload, `CSREGv1 || commitment_hash` Cell data, and—when fully
configured—a mainnet transaction intent containing the fixed output Lock, Type
Script, data, and both required code CellDeps. The publisher's wallet supplies
capacity, inputs, change, fee, witnesses, signatures, and broadcast.

The four Script configuration values are all-or-nothing:

```text
REGISTRY_TYPE_SCRIPT_JSON
REGISTRY_TYPE_SCRIPT_CELL_DEP_JSON
REGISTRY_COMMITMENT_LOCK_SCRIPT_JSON
REGISTRY_COMMITMENT_LOCK_CELL_DEP_JSON
```

`CKB_REGISTRY_SCAN_MAX_CELLS` bounds the scheduled indexer scan (default 1000,
allowed range 100–10000). `CKB_MIN_CONFIRMATIONS` defaults to 24 and applies to
deployment Cells, commitment Cells, and both configured Script code CellDeps.
Maintenance queries exact Type Script matches with a `CSREGv1` data prefix,
verifies the configured commitment Lock, and reconciles current lifecycle
state. A matching sufficiently confirmed live Cell promotes to
`on_chain_committed`; a spent or immature commitment returns to `deployed`; and
a stale deployment returns to `verification_status = verified` with
`deployment_status = undeployed` (projected as `verified_build`). Historical
evidence is retained.

Leaving all four Script values unset deliberately disables transaction-intent
construction and chain reconciliation; maintenance then clears any prior
current-commitment pointer because it can no longer re-observe that claim.
Setting only some of them is a service misconfiguration. Invalid, spent, or insufficiently confirmed code CellDeps
also fail readiness. Deploying and pinning the canonical mainnet Registry Type
and commitment Lock Scripts remains an operator action; checked-in code does
not itself prove that a public commitment exists.

## Verification Worker

The leased Postgres queue uses `FOR UPDATE SKIP LOCKED`, three-attempt bounded
retry/dead-letter handling, crash recovery, and a static-publication checkpoint.
The verifier subprocess has timeout, output, CPU, memory, process, capability,
filesystem, and temporary-storage bounds.

For CellScript source, the verifier compiles the authenticated snapshot using
the current real compiler. For generic artifact bundles it validates the
coordinate/profile and required objects, recomputes all hashes, and emits the
profile-specific verification level. Evidence insertion and the job publishing
checkpoint commit atomically; a crash after that point retries only the static
object write.

Queue operations require the admin token:

```text
GET  /v1/admin/verification-queue
POST /v1/admin/verification-jobs/:job_id/retry
```

## Admin Boundary

Admin requests require `Authorization: Bearer <REGISTRY_ADMIN_TOKEN>` or
`x-registry-admin-token`. `x-registry-admin-actor` is stored in audit events.

The generic availability endpoint accepts only `active`, `deprecated`,
`yanked`, or `quarantined`. It cannot manufacture verification or deployment
claims. Evidence-specific promotions validate required hashes and predecessor
evidence. Ordinary verified-build promotion is performed by the automatic
worker; the token-gated promotion path is for attributable recovery and
operations.

Audit events support filters for event type, principal, namespace, name,
release, time cursor, and bounded limit.

## Self-hosted Production

The checked-in stack uses Postgres 17, the Node 22 adapter, an isolated Rust
verification worker, a shared object volume, and read-only nginx. TLS is
terminated outside the compose stack. Build immutable linux/amd64 API and
verifier images before transferring them to production; do not compile the
full Rust verifier on a resource-shared production host.

```bash
cp deploy/.env.example deploy/.env
chmod 600 deploy/.env
docker compose --env-file deploy/.env -f deploy/docker-compose.production.yml config
docker compose --env-file deploy/.env -f deploy/docker-compose.production.yml up -d --no-build
```

Required runtime configuration:

```text
DATABASE_URL
REGISTRY_OBJECTS_DIR
REGISTRY_ADMIN_TOKEN
REGISTRY_ORIGIN
STATIC_REGISTRY_ORIGIN
REGISTRY_API_IMAGE
REGISTRY_VERIFIER_IMAGE
```

Mainnet deployment checks use `CKB_MAINNET_RPC_URL`. Chain commitments remain
disabled unless `REGISTRY_TYPE_SCRIPT_JSON`,
`REGISTRY_TYPE_SCRIPT_CELL_DEP_JSON`, `REGISTRY_COMMITMENT_LOCK_SCRIPT_JSON`,
and `REGISTRY_COMMITMENT_LOCK_CELL_DEP_JSON` are supplied together. Set
`REGISTRY_REPRODUCER_POLICY_JSON` before accepting reproduction promotions and
use `CKB_MIN_CONFIRMATIONS` to raise or lower the default 24-block confirmation
floor. The Node adapter, production Compose file, and Worker example pass the
same settings.

The API container applies tracked additive migrations before serving traffic.
`0001_initial.sql` is the frozen deployed baseline. `0002` adds the verifier
queue; `0003` adds multi-wallet principals; `0004` converts an empty legacy
release table to the artifact/state model and intentionally fails if rows exist
so operators cannot perform a lossy implicit migration; `0005` separates
hash-integrity evidence from semantic verification with `hash_bound`; and
`0006` admits the independent `reproduced_build` evidence kind; and `0007`
renames historical chain evidence, adds the current-commitment pointer and
status projection constraints, and deliberately demotes legacy current claims
until the mainnet indexer re-observes a sufficiently confirmed live Cell.

`GET /health` is liveness. `GET /ready` checks store/object access, admin
configuration, and—when `REQUIRE_REGISTRY_VERIFIER_READY=true`—a fresh verifier
heartbeat.

## Backups

`deploy/backup.sh` creates a Postgres custom dump, object archive, Postgres
image identity, and SHA-256 manifest under the bounded retention policy.

```bash
(cd /data/cellscript-registry/backups/<timestamp> && sha256sum --check SHA256SUMS)
docker run --rm --network none \
  -v /data/cellscript-registry/backups/<timestamp>:/backup:ro \
  postgres:17-alpine pg_restore --list /backup/postgres.dump > /dev/null
tar -tzf /data/cellscript-registry/backups/<timestamp>/objects.tar.gz > /dev/null
```

Restore rehearsals use new empty database/object volumes and require `/ready`
plus static artifact reads before traffic cut-over. Never overwrite live
volumes with an untested restore.

## Cloudflare

Configure Neon, R2, Hyperdrive, the scheduled cleanup trigger, and
`REGISTRY_ADMIN_TOKEN`; then apply migrations and deploy:

```bash
DATABASE_URL='postgres://...' npm run migrate
npm install
npm run check
npm test
npm run build
npx wrangler deploy --config wrangler.toml
```

The checked-in `wrangler.example.toml` contains no secret. Re-running
`npm run migrate` is safe because applied migration filenames are recorded in
`schema_migrations`.

## Local Verification

```bash
npm run check
npm test
npm run build
npm run build:node
cargo test --locked --manifest-path ../registry-verifier/Cargo.toml
cargo clippy --locked --manifest-path ../registry-verifier/Cargo.toml --all-targets -- -D warnings
```

The repository-wide `dev` and `ci` gates exercise these surfaces together with
the compiler, CLI, website, and independent verifier. None of the commands in
this section deploys production.
