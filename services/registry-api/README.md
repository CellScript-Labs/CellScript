# CellScript Registry API

Production API for the public CellScript registry. The same typed application
can run as a Cloudflare Worker or through the bundled Node.js HTTP adapter.

This service is the production write boundary behind:

- `https://api.registry.cellscript.dev` for authenticated writes;
- `https://registry.cellscript.dev` for static/CDN package and source-snapshot
  reads.

The Cloudflare deployment option can serve `/packages/*` and
`/source-snapshots/*` directly from R2. The current
production deployment uses the Node adapter plus a separate read-only nginx
container over the same object-store volume.

Postgres is the authoritative write store. Immutable source snapshots and
version-addressed package JSON use either R2 or the production filesystem
adapter. Package JSON is refreshed only by audited evidence/status transitions;
the source snapshot itself is content-addressed and immutable. The static read
service is intentionally separate from Postgres and the write API so accepted
package URLs remain available during a database or API incident.

The self-hosted production slice was deployed on 2026-07-31. From that point,
`migrations/0001_initial.sql` is the frozen deployed baseline; future schema
changes must be additive numbered migrations rather than edits to the initial
migration. Readiness and the public/static surfaces are available at:

```text
https://api.registry.cellscript.dev/health
https://api.registry.cellscript.dev/ready
https://registry.cellscript.dev/health
```

## Implemented Boundaries

- JoyID-rooted capability authorisation with `@joyid/ckb` `verifySignature`.
- Challenge binding against canonical `cellscript-registry-auth-v1` payloads.
- `principal_type = joyid_ckb` only.
- `principal_id` binding against the JoyID signer key; display addresses are
  not accepted as ACL keys.
- Scoped capability records with expiry and revocation fields.
- Namespace claim path with reserved/short-name review state.
- Seeded reserved namespace list for core ecosystem, hostname, security, and
  support namespaces.
- Namespace claim cooldown for newly claimed namespaces by the same JoyID
  principal; invalid JoyID signatures do not consume principal quota.
- Publish admission path for source packages.
- Single-shape `cellscript-registry-publish-v1` admission: the signed
  `registry_entry` must contain exactly the published version and explicitly
  bind Edition 2026 source semantics, its independently resolved
  compatibility-profile hash, dependencies, status, and yank state. The API
  never derives target, primitive assurance, metadata schema, or wire ABI from
  the edition year.
- Namespace owner ACL check before publish admission.
- P-256 capability-signature verification for daily publish payloads.
- One-time signed nonce consumption for capability creation, capability
  revocation, and package publish.
- `Idempotency-Key` support for package publish retries. A completed matching
  request returns the stored response with `x-idempotency-status: replayed`; the
  same key with different request content is rejected. If admission fails after
  a publish key is reserved but before the version is accepted, the processing
  reservation is released.
- Existing package versions are rejected before source snapshot writes.
- Content-addressed source snapshot and version-addressed package JSON writes
  before package-version admission; if the static read object cannot be
  persisted, the version is not accepted into the registry store.
- Static package-version JSON write to R2 at
  `/packages/:namespace/:name/versions/:version.json`; this is the direct URL
  served by `https://registry.cellscript.dev`.
- Public package-version responses include the immutable snapshot descriptor:
  URL, SHA-256 object identity, source hash, byte size, and semantic content
  type. The self-hosted static service exposes `/source-snapshots/*` read-only
  with immutable caching, allowing `cellc install` to verify and materialize
  source without cloning Git.
- Initial package-version status: `source_published`.
- Per-IP, per-ASN, per-principal, per-capability, and per-package quota hooks.
- Future `policy_hooks` and `bond_policy_hooks` tables for later bond or
  refundable-deposit policies; no on-chain fee or bond is enforced now.
- Public package index, search, package-detail, and evidence read endpoints.
- Token-gated admin operations for reserved namespaces, namespace review
  status, and conservative package-version status transitions. Generic admin
  status changes cannot claim production assurance states.
- Evidence-specific, ordered promotion from `source_published` to
  `verified_build`, `deployed`, and `on_chain_attested`. Each transition stores
  hash-addressed evidence and validates identity fields plus the preceding
  evidence reference before the status can change.
- Suppressive package-version admin transitions (`deprecated`, `yanked`,
  `quarantined`) update the static read object before changing the write-store
  status, so public reads fail conservative during incident response.
- Token-gated audit-event read path for review, incident response, and
  production debugging.
- Audit/event log records for capability, namespace, auth failure, rate-limit,
  and publish transitions, including admin review/quarantine/yank overrides.
- Successful capability use updates `last_used_at` and writes a
  `capability.used` audit event.
- Scheduled cleanup for expired nonces, idempotency records, and old quota
  events.

## Endpoints

```text
GET  /health
GET  /ready
GET  /packages/:namespace/:name/versions/:version.json
GET  /v1/packages
GET  /v1/packages/:namespace/:name
GET  /v1/packages/:namespace/:name/versions/:version/evidence
POST /v1/capabilities
POST /v1/capabilities/:key_id/revoke
POST /v1/namespaces/claim
POST /v1/packages/:namespace/:name/versions
GET  /v1/admin/audit-events
POST /v1/admin/reserved-namespaces
POST /v1/admin/namespaces/:namespace/status
POST /v1/admin/packages/:namespace/:name/versions/:version/status
POST /v1/admin/packages/:namespace/:name/versions/:version/promote
```

## Self-hosted Production Deployment

The checked-in production stack uses Postgres 17, the Node 22 adapter, a shared
object volume, and a read-only nginx service for
`registry.cellscript.dev`. It expects the external Docker network
`stack-network` to provide the TLS reverse proxy. Production TLS is terminated
by HTTPS Portal; its API-domain configuration must allow an 8 MiB request body
so the 5 MiB snapshot plus base64/JSON overhead reaches the Node adapter.

```bash
cp deploy/.env.example deploy/.env
# Generate and insert independent high-entropy database and admin secrets.
chmod 600 deploy/.env
docker compose --env-file deploy/.env -f deploy/docker-compose.production.yml config
docker compose --env-file deploy/.env -f deploy/docker-compose.production.yml up -d --build
```

The API container applies tracked migrations before it starts accepting
traffic. Postgres is reachable only on the internal network. The API and static
services run with read-only root filesystems, bounded temporary filesystems,
health checks, log rotation, and `no-new-privileges`.

Production validation performed at deployment includes trusted TLS for both
domains, dependency-aware readiness, a 2 MiB request reaching application JSON
validation, a structured application 413 at 7 MiB + 1 byte, rejection of
unauthorised admin writes and static POSTs, path-traversal rejection, API
restart recovery, and persistent audit/database/object volumes.

Required runtime configuration:

```text
DATABASE_URL
REGISTRY_OBJECTS_DIR
REGISTRY_ADMIN_TOKEN
REGISTRY_ORIGIN
STATIC_REGISTRY_ORIGIN
```

`MAX_INCOMING_BODY_BYTES` limits the Node adapter before the request reaches
the application parser. Keep it slightly larger than `MAX_JSON_BODY_BYTES`,
which must in turn cover the base64 representation of `MAX_SNAPSHOT_BYTES`.

## Production Backups

`deploy/backup.sh` creates one atomic backup directory containing:

- a custom-format, owner-free Postgres dump;
- a gzip archive of the object volume, captured after the database snapshot so
  every object referenced by that database dump is present;
- the Postgres image identity; and
- SHA-256 checksums for all three files.

The default destination is `/data/cellscript-registry/backups`, and only
timestamp-shaped backup directories older than the bounded retention window are
removed. The default retention is seven days and may be set from 1 to 365 with
`REGISTRY_BACKUP_RETENTION_DAYS`.

The checked-in systemd service/timer runs this backup daily with a randomized
delay and a restricted filesystem view:

```bash
install -d -m 0750 /data/cellscript-registry/backups
install -m 0644 deploy/cellscript-registry-backup.service /etc/systemd/system/
install -m 0644 deploy/cellscript-registry-backup.timer /etc/systemd/system/
systemctl daemon-reload
systemctl enable --now cellscript-registry-backup.timer
systemctl start cellscript-registry-backup.service
```

Verify a backup before treating it as recoverable:

```bash
(cd /data/cellscript-registry/backups/<timestamp> && sha256sum --check SHA256SUMS)
docker run --rm --network none \
  -v /data/cellscript-registry/backups/<timestamp>:/backup:ro \
  postgres:17-alpine pg_restore --list /backup/postgres.dump > /dev/null
tar -tzf /data/cellscript-registry/backups/<timestamp>/objects.tar.gz > /dev/null
```

A restore rehearsal uses new empty database/object volumes, restores the dump
and object archive, then requires `/ready` plus static package reads before any
traffic cut-over. Do not overwrite the live volumes as an untested restore.

## Cloudflare Deployment

1. Create a Neon Postgres database.
2. Apply database migrations:

```bash
DATABASE_URL='postgres://...' npm run migrate
```

3. Create a Cloudflare R2 bucket for source snapshots and static registry JSON
   objects.
4. Create a Cloudflare Hyperdrive config pointing at Neon.
5. Copy `wrangler.example.toml` to `wrangler.toml`.
6. Replace `REPLACE_WITH_CLOUDFLARE_HYPERDRIVE_ID`.
7. Confirm `[triggers]` is enabled in `wrangler.toml`; the example schedules a
   cleanup run every 15 minutes.
8. Configure admin auth as a Cloudflare secret:

```bash
npx wrangler secret put REGISTRY_ADMIN_TOKEN --config wrangler.toml
```

9. Deploy with:

```bash
npm install
npm run check
npm test
npm run build
npx wrangler deploy --config wrangler.toml
```

`wrangler.example.toml` is intentionally safe to commit. The real
`wrangler.toml` should not contain secrets; secrets must be configured through
Cloudflare bindings/secrets.

`npm run migrate` creates and uses a local `schema_migrations` table. Re-running
it is safe; already-applied migration files are skipped.

`GET /health` is a process liveness check. `GET /ready` performs live store and
object-adapter checks, including write access to both managed
`source-snapshots` and `packages` prefixes, and returns `503` until every
required dependency and the admin token are ready. The production volume
initializer repairs ownership and directory/file modes recursively before the
API starts. `NAMESPACE_CLAIM_COOLDOWN_SECONDS` defaults to `3600`; lower it
only for controlled staging tests.

## Admin Governance Boundary

Admin operations require `Authorization: Bearer <REGISTRY_ADMIN_TOKEN>` or
`x-registry-admin-token`. The optional `x-registry-admin-actor` header is stored
in audit logs so manual review, reserved namespace changes, quarantine, yanks,
and deprecations are attributable.

Supported package-version status transitions through the admin API are:

```text
source_published
indexed_pending
deprecated
yanked
quarantined
```

`verified_build`, `deployed`, and `on_chain_attested` are accepted only through
the evidence endpoint. A verified build binds source, manifest, compatibility
profile, artifact, metadata, and compiler version. Deployment evidence must
reference that verified-build evidence and prove the same artifact is live at
a concrete CKB out point. On-chain attestation must in turn reference the
accepted deployment evidence and record a confirmed attestation transaction.

Audit events can be queried with:

```text
GET /v1/admin/audit-events?event_type=namespace.claimed&namespace=cellscript&limit=50
```

The endpoint requires the same admin token and supports filters for
`event_type`, `principal_type`, `principal_id`, `namespace`, `name`, `version`,
`before`, and `limit`. `limit` is capped at 200.

## Capability Registration And Revocation

`cellc auth capability create` only creates the local delegated key and prints
the JoyID challenge. It does not register the key until the JoyID-signed payload
is submitted to the write API:

```bash
cellc auth capability create --principal-id <principal_id> --scope publish:ns/pkg --expires 90d --json > capability-payload.json
# Sign capability-payload.json with the production JoyID path exposed through CCC.
cellc auth capability submit --payload capability-payload.json --joyid-signature joyid-signature.json
cellc auth namespace claim --namespace ns --payload capability-payload.json --joyid-signature joyid-signature.json
```

The registry submit page can sign the same payload through the CCC JoyID CKB
signer and submit it directly to `/v1/capabilities`. The signed response can
also be copied as `joyid-signature.json` for the CLI submit path.
The separate **Claim namespace** action, or `cellc auth namespace claim`, sends
the same signed authorisation to `/v1/namespaces/claim`. A first publish is
intentionally rejected until that claim is active; reserved namespaces may
remain pending for administrator review.

The submit page derives the preferred `principal_id` from the connected JoyID
signer and exposes a copy action. The API verifies that the JoyID signature's
public key and key type match the `principal_id` embedded in the payload before
recording the capability.

Capability revocation follows the same challenge/submit shape so that the
revocation is also bound to the JoyID root principal:

```bash
cellc auth capability revoke --principal-id <principal_id> --capability-key-id <capability_key_id> --json > revoke-payload.json
# Sign revoke-payload.json with JoyID.
cellc auth capability revoke --payload revoke-payload.json --joyid-signature joyid-signature.json --reason "rotate delegated key"
```

## Publish Payload Boundary

Capability creation signs the canonical JSON form of:

```text
cellscript-registry-auth-v1 / authorize_capability
```

Daily publish signs the canonical JSON form of:

```text
cellscript-registry-publish-v1 / publish
```

The API rejects a publish unless:

- the capability exists;
- the capability is unrevoked and unexpired;
- the capability scope covers `publish:namespace/package`;
- the namespace exists and is active;
- the capability principal owns the namespace;
- the signed nested registry entry uses the current schema, names the same package/version
  and source hash, and records `edition = "2026"` plus a 32-byte
  `compatibility_profile_hash`; edition identifies source semantics, while the
  hash commits to the complete target/assurance/ABI/schema combination;
- the signed manifest hash is present;
- the capability signature verifies;
- the signed publish nonce has not already been consumed;
- the package version does not already exist;
- a source snapshot is provided and persisted to the configured object store;
- a static package-version JSON object is persisted for the read-only path.

Clients that need safe retry semantics should send an `Idempotency-Key` header
with at least 16 visible token characters. The key is not an auth credential; it
only scopes response replay and conflict detection for the exact publish
request body.

`cellc publish` sends this header by default using a hash of the exact publish
request. It can be pinned with `--idempotency-key` or
`CELLSCRIPT_REGISTRY_IDEMPOTENCY_KEY` for CI jobs that intentionally retry the
same request.

If publish admission fails before the package version is accepted, the write API
releases both the matching `processing` idempotency reservation and the nonce
record created by that request. The exact signed request can therefore be
retried safely. Package, snapshot, version, capability-use, acceptance-audit,
and completed-idempotency records commit in one database transaction; immutable
object writes happen before that transaction and may be repeated safely.

Successful publish returns a direct static read URL shaped as:

```text
https://registry.cellscript.dev/packages/:namespace/:name/versions/:version.json
```

The route is served from the object store and sets short cache headers. It does
not require Postgres or the write store, so ordinary package reads stay isolated
from authenticated write-path dependencies. Its JSON object repeats `edition`
and `compatibility_profile_hash` at the top level so consumers do not need to
trust an untyped nested blob and do not have to overload the edition label with
ABI or schema meaning.

The same static origin serves the content-addressed `source_snapshot.url`
reported in that JSON. Generated CellScript snapshots use
`application/vnd.cellscript.source-snapshot+json`; the resolver rejects opaque
archive types, unsafe/duplicate paths, incorrect per-file hashes, a wrong
package coordinate, or a mismatched whole-tree source hash.

CLI publish has two supported signing shapes:

```bash
# Daily local use: key was generated by auth capability create and stored in keychain.
cellc publish

# CI or external signer: sign the canonical payload, then submit it unchanged.
cellc publish --print-payload --json > publish-payload.json
cellc publish --payload publish-payload.json --capability-signature <signature>
```

`CELLSCRIPT_REGISTRY_API_URL` overrides the write API base URL. CI may set
`CELLSCRIPT_CAPABILITY_PRIVATE_KEY_PKCS8_B64` to let the CLI sign with a
delegated capability key without JoyID or keychain access.
`CELLSCRIPT_REGISTRY_IDEMPOTENCY_KEY` pins the publish retry key; otherwise the
CLI derives one from the publish request and reuses it for transient retry of
the same HTTP submission.

## Local Verification

```bash
npm run check
npm test
npm run build
```

`npm run build` performs a wrangler dry-run bundle against the example
configuration. It does not deploy.
