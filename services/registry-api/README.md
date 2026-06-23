# CellScript Registry API

Cloudflare Workers write API for the public CellScript registry.

This service is the production write boundary behind:

- `https://api.registry.cellscript.dev` for authenticated writes;
- `https://registry.cellscript.dev` for static/CDN reads.

The same Worker can also serve `https://registry.cellscript.dev/packages/*`
directly from R2, while the rest of the website may stay on Pages/static
hosting.

It intentionally does not use D1 as the primary database. Runtime state is
stored in Neon Postgres through Cloudflare Hyperdrive, while immutable source
snapshots and static registry read objects are stored in R2.

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
- Immutable R2 source snapshot and static package-version JSON writes before
  package-version admission; if the static read object cannot be persisted, the
  version is not accepted into the registry store.
- Static package-version JSON write to R2 at
  `/packages/:namespace/:name/versions/:version.json`; this is the direct URL
  served by `https://registry.cellscript.dev`.
- Initial package-version status: `source_published`.
- Per-IP, per-ASN, per-principal, per-capability, and per-package quota hooks.
- Future `policy_hooks` and `bond_policy_hooks` tables for later bond or
  refundable-deposit policies; no on-chain fee or bond is enforced now.
- Token-gated admin operations for reserved namespaces, namespace review
  status, and package-version status transitions.
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
POST /v1/capabilities
POST /v1/capabilities/:key_id/revoke
POST /v1/namespaces/claim
POST /v1/packages/:namespace/:name/versions
GET  /v1/admin/audit-events
POST /v1/admin/reserved-namespaces
POST /v1/admin/namespaces/:namespace/status
POST /v1/admin/packages/:namespace/:name/versions/:version/status
```

## Deploy Setup

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

`GET /health` is a liveness check. `GET /ready` is the production readiness
check and returns `503` until Hyperdrive, R2, and `REGISTRY_ADMIN_TOKEN` are all
configured. `NAMESPACE_CLAIM_COOLDOWN_SECONDS` defaults to `3600`; lower it only
for controlled staging tests.

## Admin Governance Boundary

Admin operations require `Authorization: Bearer <REGISTRY_ADMIN_TOKEN>` or
`x-registry-admin-token`. The optional `x-registry-admin-actor` header is stored
in audit logs so manual review, reserved namespace changes, quarantine, yanks,
deprecations, and verification promotions are attributable.

Supported package-version status transitions through the admin API are:

```text
source_published
indexed_pending
verified_build
deployed
deprecated
yanked
quarantined
```

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
```

The registry submit page can sign the same payload through the CCC JoyID CKB
signer and submit it directly to `/v1/capabilities`. The signed response can
also be copied as `joyid-signature.json` for the CLI submit path.

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
- the capability signature verifies;
- the signed publish nonce has not already been consumed;
- the package version does not already exist;
- a source snapshot is provided and persisted to R2;
- a static package-version JSON object is persisted to R2 for the CDN read path.

Clients that need safe retry semantics should send an `Idempotency-Key` header
with at least 16 visible token characters. The key is not an auth credential; it
only scopes response replay and conflict detection for the exact publish
request body.

`cellc publish` sends this header by default using a hash of the exact publish
request. It can be pinned with `--idempotency-key` or
`CELLSCRIPT_REGISTRY_IDEMPOTENCY_KEY` for CI jobs that intentionally retry the
same request.

If publish admission fails before the package version is accepted, the write API
releases the matching `processing` idempotency reservation. The signed publish
nonce may already have been consumed, so a later retry with the same CI retry key
must use a freshly generated publish payload and capability signature.

Successful publish returns a direct static read URL shaped as:

```text
https://registry.cellscript.dev/packages/:namespace/:name/versions/:version.json
```

The route is served from R2 and sets short CDN cache headers. It does not
require Hyperdrive or the write store, so ordinary package reads stay isolated
from authenticated write-path dependencies.

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
