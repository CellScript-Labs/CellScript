# Tutorial 12: Registry Artifacts End to End

**Status**: current tutorial for publishing and inspecting CellScript and
non-CellScript artifacts in the public Registry.

The Registry is not limited to dependency packages. It distinguishes source
libraries, profile libraries, CKB runtime verifiers, deployable contracts,
reproducible binaries, and copy-only templates. This tutorial uses the native
CellScript path first, then the generic artifact path.

## 1. Connect a CKB wallet

Open `https://cellscript.dev/registry/submit`. The page does not expose a
network selector: Registry authorisation and deployment evidence are CKB
mainnet-only.

Choose a detected wallet from the modal. Wallets listed without an active
connector link to their official installation page. The wallet signs only the
canonical capability authorisation; `cellc` generates and stores the delegated
P-256 publish key.

Claim a namespace and wait until it is active. The submit form then produces
the capability and publish commands for the selected artifact kind.

## 2. Publish a CellScript source library

Add the namespace to `Cell.toml`:

```toml
[package]
name = "math"
version = "1.0.0"
namespace = "acme"
```

Verify and publish:

```bash
cellc package verify --json
cellc publish --dry-run
cellc publish
```

Use `--artifact-kind profile_library` when the package is a named CellScript
profile library. Both kinds use compiler-backed verification and remain valid
`Cell.toml` dependencies.

## 3. Publish a deployable CKB contract

Create `Artifact.toml`:

```toml
schema = "cellscript-registry-artifact"
namespace = "acme"
name = "vault-lock"
release = "1.0.0"
kind = "deployable_contract"
language = "rust"
bundle = "vault-lock.bundle.json"
description = "Vault lock Script"
```

Create a closed profile contract first. Its ABI hash is the CKB Blake2b-256 of
the immutable ABI object:

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
    "abi_hash": "<ABI object CKB Blake2b-256>"
  }
}
```

Canonicalize it recursively by key and put that JSON string in the immutable
bundle. Each payload is base64-encoded bytes, not a path:

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

Validate before sending anything:

```bash
cellc publish --artifact-manifest Artifact.toml --dry-run
```

The CLI checks the coordinate, release, kind/language pair, bundle profile,
required object roles, size limit, and computed hashes. Publish with:

```bash
cellc publish --artifact-manifest Artifact.toml
```

The release initially reports:

```text
verification_status = pending
deployment_status   = undeployed
availability_status = active
```

After the independent verifier binds the source, executable, ABI, and profile
contract hashes, verification becomes `hash_bound`. That is an integrity claim,
not a claim about Script semantics, security review, or deployment.

## 4. Record a mainnet deployment

The deployment request is a signed
`cellscript-registry-deployment` / `record_deployment` payload sent to:

```text
POST /v1/artifacts/acme/vault-lock/releases/1.0.0/deployments
```

It includes the published `artifact_hash`, equal `data_hash`, `code_hash`,
`hash_type`, `dep_type`, and the mainnet OutPoint. The API requires the same
namespace capability used for publishing and prior verified-build evidence.

The API calls mainnet `get_live_cell`. It rejects a dead or missing Cell, a
data-hash mismatch, a Type Script hash mismatch, a non-mainnet network, or an
OutPoint that is not bound to the published executable. A successful request
appends deployment evidence and changes only `deployment_status` to
`chain_verified`.

For a DepGroup OutPoint, the API decodes the live Cell data as the canonical
Molecule `OutPointVec` and finds the matching live code member. It does not hash
the DepGroup container as though it were the executable.

## 5. Inspect the artifact

Open the artifact detail page or query the API:

```bash
curl --fail 'https://api.registry.cellscript.dev/v1/artifacts/acme/vault-lock'
curl --fail 'https://api.registry.cellscript.dev/v1/artifacts/acme/vault-lock/releases/1.0.0/evidence'
```

Check these independently:

- artifact kind, profile, language, and consumption mode;
- source, executable, ABI, or recipe hashes;
- verification, deployment, and availability states;
- evidence producer and evidence hash;
- mainnet OutPoint, code hash, data hash, hash type, and dep type.

Do not use `cellc install` for this executable. `cellc install` accepts only
`cellscript_source` artifacts whose consumption mode is `dependency`.

Consume it explicitly:

```bash
cellc artifact fetch acme/vault-lock@1.0.0 --output vault-lock.bundle.json
cellc artifact verify --bundle vault-lock.bundle.json --receipt vault-lock.bundle.json.receipt.json
cellc artifact pin acme/vault-lock@1.0.0 --output Artifacts.lock --accept-hash-bound
cellc artifact record-deployment acme/vault-lock@1.0.0 --code-hash <hash> --hash-type data1 --dep-type code --tx-hash <tx_hash> --index 0 --capability-key-id <key_id>
cellc artifact cell-dep acme/vault-lock@1.0.0 --output CellDep.json --accept-hash-bound --rpc-url https://mainnet.ckb.dev/rpc
cellc artifact set-availability acme/vault-lock@1.0.0 --status yanked --reason "security advisory" --capability-key-id <key_id>
cellc artifact commitment acme/vault-lock@1.0.0 --output RegistryCommitment.json
```

`cell-dep` fails until mainnet deployment evidence has been verified, then
rechecks that the deployment (and resolved DepGroup code member) is still live
at consumption time. Deployment mode must equal the immutable profile
contract. The commitment file contains canonical `CSREGv1` Cell data; attestation still
requires the API to read a live mainnet Cell and match its Type/Lock identities.

## 6. Other artifact kinds

- `runtime_verifier`: `ckb_executable` bundle with source, executable, and ABI;
  consumption mode is `tcb`.
- A `ckb_executable` that is built reproducibly may additionally include
  `build_recipe`, set `build.reproducible = true`, and bind the recipe,
  environment, command, and expected executable hash in `reproduction`.
- `reproducible_binary`: `reproducible_build` bundle with source, executable,
  and `build_recipe`; the Registry reports `evidence_required` until build
  evidence is sufficient.
- `template`: `copy_material` bundle containing a
  `cellscript-template-file-map-v1` source object; use `cellc artifact copy`.
  It rejects traversal, duplicates, hash drift, and overwrites.

An artifact declaring `security.status = "audited"` must also carry an
immutable `audit_report` bundle object whose CKB Blake2b-256 hash exactly
matches `security.audit_report_hash`. This authenticates the referenced report;
it does not make the Registry the auditor.

## 7. Naming rules

Namespace and artifact names are 1–64 characters. Use lowercase letters and
digits; `_` and `-` may appear only between characters. A one-character name is
valid. The UI and API enforce the same rule.

## 8. Validate repository integration

```bash
./scripts/cellscript_gate.sh dev
```

For the complete model and failure rules, see
[`docs/CELLSCRIPT_REGISTRY_PHASE1.md`](../CELLSCRIPT_REGISTRY_PHASE1.md).
