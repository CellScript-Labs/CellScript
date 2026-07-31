# Tutorial 12: Phase 1 Registry: End-to-End

This tutorial walks through the Phase 1 registry loop at the level a package
author or reviewer needs: source identity, build identity, deployment identity,
and the commands that bind them together.

For the longer repository version, read
[docs/tutorials/phase1-end-to-end.md](https://github.com/CellScript-Labs/CellScript/blob/main/docs/tutorials/phase1-end-to-end.md).

The production surfaces are live:

```text
Website:      https://cellscript.dev/registry/
Public API:   https://api.registry.cellscript.dev/v1/packages
Write API:    https://api.registry.cellscript.dev
Static reads: https://registry.cellscript.dev/packages/
```

Package browsing is live-data-first. If the API is unavailable, the website
labels its bundled fixture as a read-only mirror; it is never the write or
resolution authority.

## What Phase 1 Proves

Phase 1 is not a chain acceptance test and not a trust oracle. It answers three
bounded questions:

| Question | Evidence |
| --- | --- |
| Which source was published? | `Cell.toml`, package source hash, namespace/name/version, registry metadata. |
| Which build came from that source? | Artifact hash, metadata hash, ABI/schema/constraint hashes, compiler version, target profile. |
| Which deployed Cell claims to contain that build? | Network, tx hash, output index, code hash, data hash, CellDep/deployment metadata. |

The rule is fail-closed. Missing hashes, stale source, toolchain drift, or a
deployment record that does not match chain facts should be treated as a
verification failure.

## Author Flow

Start with a package:

```bash
cellc init my_contract
cd my_contract
```

Fill in the package identity in `Cell.toml`: name, namespace, version,
description, repository, license, entry file, and target profile. Then write the
source and build it:

```bash
cellc check --target-profile ckb --json
cellc build --target riscv64-elf --target-profile ckb --json
```

Before publishing, do a local dry run:

```bash
cellc publish --dry-run --json
```

For an offline mirror or release fixture, write local registry metadata:

```bash
cellc publish --offline --json
```

For public publishing, authorize a local publisher capability through the JoyID
flow, then publish:

```bash
cellc auth capability create --principal-id <principal_id> --scope publish:cellscript/amm_pool --expires 90d --json > capability-payload.json
cellc auth capability submit --payload capability-payload.json --joyid-signature joyid-signature.json
cellc auth namespace claim --namespace cellscript --payload capability-payload.json --joyid-signature joyid-signature.json
cellc publish --json
```

Namespace ownership is an explicit admission step, not a side effect of
capability registration. The claim must be `active` before the first publish;
reserved namespaces may return a pending review status.

The public write API admits package metadata, but consumers still verify the
source and build identity locally.

## Source Edition And Compatibility Profile Contract

Public publishing uses the registry's single current publish contract. The
signed payload contains one complete version entry, and the API checks that
its namespace, package name, version, and source hash equal the outer signed
identity. The entry must also contain:

```json
{
  "schema_version": 1,
  "versions": [{
    "edition": "2026",
    "compatibility_profile_hash": "<32-byte hex hash>"
  }]
}
```

These are not website labels. `edition` identifies source-language semantics;
`compatibility_profile_hash` separately binds the complete combination of
edition, target, primitive assurance, metadata schemas, and entry/witness ABI.
The API stores both as typed fields and exposes them in its static
package-version JSON. Consumers must not derive ABI or schema versions from the
edition year. Missing `edition`, `compatibility_profile_hash`,
`dependencies`, `status`, or `yanked`, an unknown schema identifier, or a
mismatched nested identity is rejected. The production Registry deployed this
as its initial schema on 2026-07-31; `0001_initial.sql` is now frozen and later
schema changes require additive migrations. `0002_verification_jobs.sql` is the
first such migration and adds the automatic queue without rewriting history.

`source_published` means the signed source snapshot was admitted; it does not
mean the build or deployment was verified. The generic admin endpoint cannot
promote an entry to `verified_build`, `deployed`, or `on_chain_attested`.
Those labels require the ordered evidence endpoint. Each step stores
hash-addressed evidence, validates the package/build identity, and binds the
next step to the preceding evidence reference.

The baseline `verified_build` step is automatic. Publish creates a verification
job in the same database transaction as the version. A leased worker then
authenticates the immutable generated snapshot, compiles it with the current
CellScript compiler, verifies the canonical manifest and resolved-profile
hashes, commits evidence/status atomically, and refreshes the static version
object. Queue attempts are bounded; rejected builds dead-letter, while
operators can inspect metrics and audit an explicit requeue. Admission therefore
returns `verification: queued`, never a synchronous verification claim.

The worker is live in the production topology as of 2026-08-01. Deployment
acceptance used an explicitly seeded one-time smoke identity to exercise the
normal external `cellc publish` path, queue lease, real compiler, evidence
commit, static publication, default visibility, and a fresh consumer
install/check/build without an unverified override. The test records and live
objects were removed afterward, and the migrated clean state was backed up.
This validates the deployed automation; it deliberately does not count as a
publisher-owned JoyID capability registration or namespace claim.

## Consumer Flow

Add a dependency, resolve it, and check the resulting package graph:

```bash
cellc install namespace/package@1.2.3
cellc install
cellc package verify --json
```

The default resolver queries the production public API, accepts only statuses
eligible for normal resolution, then downloads the version's content-addressed
source snapshot. It verifies the snapshot descriptor's SHA-256, rejects opaque
or path-escaping content, verifies every file's BLAKE2b digest, reconstructs the
source tree atomically, and checks `Cell.toml`, source hash, Edition 2026, and
compatibility-profile identity.
The default package list/search follows the same baseline and shows only
`verified_build`, `deployed`, or `on_chain_attested`. Direct package/version
URLs and an explicit `?status=source_published` query remain available for
auditing an admitted version before verification completes.
For a direct `source_published` or `indexed_pending` install, pass
`--allow-unverified`; incident review of a quarantined entry additionally needs
`--allow-quarantined`. `cellc install` persists these acknowledgements on that
dependency's `Cell.toml` table, so lock refreshes and later builds retain the
same explicit policy.
`CELLSCRIPT_REGISTRY_URL` is an explicit Git/offline override, not an automatic
fallback from a failed production lookup. Registry packages otherwise use the
same fail-closed principle as path and Git dependencies: the selected source
must match the recorded identity before the compiler can treat it as part of
the build.

Then build and verify the artifact:

```bash
cellc build --target riscv64-elf --target-profile ckb --json
cellc verify-artifact build/main.elf --expect-target-profile ckb --verify-sources --production
```

## Deployment Review

After a deployment adapter records chain facts, verify the local deployment
metadata:

```bash
cellc registry verify --json
```

If you have a CKB RPC endpoint and want live chain checks:

```bash
cellc registry verify --live --rpc-url "$CELLSCRIPT_CKB_RPC_URL" --json
```

Live checks do not replace source/build verification. They add the chain-facing
question: does the recorded OutPoint still expose the expected deployment
identity?

## What Not To Put In The Resolver

The registry may discover more than the resolver can import. Keep these
boundaries separate:

| Object | Correct treatment |
| --- | --- |
| CellScript source package | `Cell.toml` dependency, resolved by `cellc install`. |
| Deployed verifier or helper script | Deployment/verifier evidence with code hash, data hash, OutPoint, ABI, and status. |
| Reproducible CKB binary | Future artifact profile, not a source package. |
| Protocol skeleton or cookbook | Copy into local source; after copying, verify as your own package. |

A useful repository is not automatically an installable dependency. A cookbook
is starting material, not registry-trusted source identity.

## Failure Modes To Expect

Phase 1 should reject:

- source files that no longer hash to the published source identity;
- `Cell.lock` or deployment metadata that names a different build;
- missing compiler, target profile, ABI, schema, or constraints hashes;
- deployment records with mismatched network, tx hash, output index, code hash,
  or data hash;
- production verification that still depends on unresolved runtime obligations.

## See Also

- [Packages and CLI Workflow](Tutorial-04-Packages-and-CLI-Workflow.md)
- [Metadata, Verification, and Production Gates](Tutorial-06-Metadata-Verification-and-Production-Gates.md)
- [CKB Target Profiles](Tutorial-05-CKB-Target-Profiles.md)
- [Agentic Loops and cellscript-mcp](Tutorial-13-Agentic-Loops-and-cellscript-mcp.md)
- `docs/CELLSCRIPT_PACKAGE_PROVENANCE_AND_DEPLOYMENT_IDENTITY.md`
- `docs/CELLSCRIPT_REGISTRY_PHASE1.md`
