# Phase 1 Registry: End-to-End

This tutorial walks through the Phase 1 registry loop at the level a package
author or reviewer needs: source identity, build identity, deployment identity,
and the commands that bind them together.

For the longer repository version, read
[docs/tutorials/phase1-end-to-end.md](https://github.com/CellScript-Labs/CellScript/blob/main/docs/tutorials/phase1-end-to-end.md).

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
cellc auth capability create --principal-id joyid:example --scope publish:cellscript/amm_pool --expires 90d --json > capability-payload.json
cellc auth capability submit --payload capability-payload.json --joyid-signature joyid-signature.json
cellc publish --json
```

The public write API admits package metadata, but consumers still verify the
source and build identity locally.

## Consumer Flow

Add a dependency, resolve it, and check the resulting package graph:

```bash
cellc add math --git https://example.com/math.git
cellc install
cellc package verify --json
```

Registry packages use the same fail-closed principle as path and Git
dependencies: the selected source must match the recorded identity before the
compiler can treat it as part of the build.

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

- [Packages and CLI Workflow](Tutorial-04-Packages-and-CLI-Workflow)
- [Metadata, Verification, and Production Gates](Tutorial-06-Metadata-Verification-and-Production-Gates)
- [CKB Target Profiles](Tutorial-05-CKB-Target-Profiles)
- `docs/CELLSCRIPT_PACKAGE_PROVENANCE_AND_DEPLOYMENT_IDENTITY.md`
- `docs/CELLSCRIPT_REGISTRY_PHASE1.md`
