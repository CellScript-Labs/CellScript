# Tutorial 15: LS-IDL for CKB Lock Scripts

**Status**: current CellScript 0.24 workflow for LS-IDL 0.1 Lock Script
interfaces.

LS-IDL describes the witness fields expected by a CKB Lock Script. CellScript
Registry preserves that description as exact bytes and binds it to the
deployed executable with this commitment:

```text
code Cell data = executable bytes || SHA-256(raw idl.json bytes)
```

This tutorial follows the complete implemented workflow:

```text
raw idl.json
    -> cellc validate
    -> cellc bind to a clean CKB executable
    -> cellc bundle and publish
    -> deploy the bound executable
    -> record chain evidence
    -> cellc fetch the exact IDL by Script identity
```

The word **raw** matters. Whitespace, key order, and the final newline are part
of the committed IDL identity. Parsing and reserialising equivalent JSON can
produce different bytes and therefore a different SHA-256 digest.

## What `cellc` Does

The LS-IDL command group is:

```bash
cellc artifact ls-idl <validate|bind|bundle|fetch>
```

It provides four bounded operations:

- `validate` checks the supported LS-IDL 0.1 schema and reports the SHA-256 of
  the exact file bytes;
- `bind` appends that 32-byte digest to a CKB executable;
- `bundle` creates the immutable Registry bundle and `Artifact.toml`; and
- `fetch` resolves an IDL by a chain-verified CKB Script identity, validates
  the Registry response contract, and writes the exact response bytes.

`cellc` does not derive `idl.json` from a Rust type, compile an external Rust,
C, or JavaScript Lock Script, deploy a code Cell, or sign a deployment
transaction. Use
[`ckb-idl-derive`](https://github.com/OWK50GA/ckb-idl-derive),
[`ckb-idl-client`](https://github.com/OWK50GA/ckb-idl-client), and the Lock
Script's own build and deployment workflow for those steps. The executable
passed to `cellc` must already be a real CKB RISC-V artifact.

The checked-in walkthrough inputs are under
[`examples/registry_ls_idl`](../../examples/registry_ls_idl/README.md).
`lock.rs` and `idl.json` show the derive-to-document relationship, but the
example intentionally does not pretend to be an audited or deployable Lock
Script.

## Prerequisites

Prepare all of the following before publishing:

- CellScript 0.24 `cellc`;
- the original LS-IDL 0.1 `idl.json`;
- a clean, unbound CKB Lock Script executable;
- the source file to include in the Registry bundle;
- an immutable 40- or 64-hex source revision;
- the intended CKB `hash_type` and `dep_type`; and
- a compatible CKB wallet for Registry writes and deployment.

Validation and lookup are public read operations and do not require a wallet.
Wallet authorisation is required when publishing or attaching deployment
evidence.

## 1. Prepare the IDL Document

A small document looks like this:

```json
{
  "idl_version": "0.1",
  "name": "demo_lock",
  "witness": [
    {
      "name": "signature",
      "type": "secp256k1_sig",
      "required": true,
      "description": "Recoverable CKB secp256k1 signature"
    },
    {
      "name": "nonce",
      "type": "uint64",
      "required": true
    },
    {
      "name": "memo",
      "type": "bytes",
      "required": false
    }
  ]
}
```

The Registry profile accepts a non-empty JSON object no larger than 256 KiB.
`witness` is required and may contain at most 256 fields. Field names must be
unique.

The implemented field types are:

| LS-IDL type | Linear encoding |
|---|---|
| `uint8` | one unsigned byte |
| `uint32` | four-byte little-endian unsigned integer |
| `uint64` | eight-byte little-endian unsigned integer |
| `secp256k1_sig` | 65 bytes |
| `secp256k1_pubkey` | 33 bytes |
| `schnorr_sig` | 64 bytes |
| `bytes` | four-byte little-endian length followed by the payload |

The optional top-level fields are `idl_version`, `name`, `description`,
`script_version`, and `signing`. When present, `signing` contains exactly the
non-empty string fields `algorithm`, `message`, and `hasher`.

In LS-IDL 0.1, `required: false` is descriptive interface metadata. The current
linear decoder has no presence bitmap and does not skip that field on the
wire. Do not use `required: false` as an optional-field encoding rule.

## 2. Validate the Exact Bytes

Run validation before touching the executable:

```bash
cellc artifact ls-idl validate --idl idl.json --json
```

The JSON result includes:

```json
{
  "status": "valid",
  "format": "ls-idl",
  "format_version": "0.1",
  "sha256": "<SHA-256 of the exact idl.json bytes>",
  "executable_suffix_bound": false
}
```

This step rejects malformed JSON, unknown keys, unsupported types, duplicate
field names, missing required field properties, and profile budget violations.
It does not modify the IDL or executable.

Treat the reported digest as part of the release identity. If `idl.json`
changes afterward, even only in formatting, repeat validation and binding.

## 3. Bind the IDL to the Executable

Always bind from a clean build output:

```bash
cellc artifact ls-idl bind \
  --idl idl.json \
  --executable build/demo-lock \
  --output build/demo-lock.ls-idl \
  --json
```

`bind` validates the IDL, computes `SHA-256(raw idl.json bytes)`, and writes:

```text
build/demo-lock bytes || 32-byte IDL digest
```

It never silently overwrites an existing output. Choose a new output path or
pass `--force` only when replacing that exact intended file. If the input
already ends with the same digest, `bind` does not append it a second time.

Do not repeatedly bind updated IDLs onto an already bound artifact. Rebuild or
return to the clean executable, then bind the new IDL once. The bound output is
the artifact that must be tested, hashed, deployed, and published; the original
unbound executable has a different CKB data hash and is not the registered
LS-IDL artifact.

Verify the final pair explicitly:

```bash
cellc artifact ls-idl validate \
  --idl idl.json \
  --executable build/demo-lock.ls-idl \
  --json
```

The result now reports `executable_suffix_bound: true`. A suffix mismatch is a
hard error.

## 4. Create the Registry Bundle

Create the immutable bundle only from the bound executable:

```bash
cellc artifact ls-idl bundle \
  --idl idl.json \
  --executable build/demo-lock.ls-idl \
  --source src/lib.rs \
  --namespace example \
  --name demo-lock \
  --release 0.1.0 \
  --language rust \
  --hash-type data1 \
  --dep-type code \
  --toolchain 'rustc 1.97.1 + ckb-std' \
  --source-revision <40-or-64-hex-immutable-revision> \
  --output artifact.bundle.json \
  --artifact-manifest-output Artifact.toml \
  --json
```

The accepted `--language` values are `cellscript`, `rust`, `c`, `javascript`,
and `other`. `--hash-type` defaults to `data1`; `--dep-type` defaults to
`code`. State both explicitly in release automation so deployment evidence
cannot inherit an accidental default.

The command writes two files:

- `artifact.bundle.json` contains exactly one `source`, `executable`, and `abi`
  object as Base64-encoded bytes; and
- `Artifact.toml` names the `deployable_contract` release and points to that
  bundle.

It also reports four different identities:

| Output field | Algorithm and object | Purpose |
|---|---|---|
| `source_hash` | CKB Blake2b-256 of the source object | immutable source-object identity |
| `artifact_hash` | CKB Blake2b-256 of the bound executable | CKB executable/data identity |
| `abi_hash` | CKB Blake2b-256 of the raw IDL object | Registry ABI object identity |
| `idl_sha256` | SHA-256 of the raw IDL object | LS-IDL executable-suffix commitment |

Do not substitute one hash for another. In particular, LS-IDL uses SHA-256 for
the suffix even though the Registry artifact and ABI objects also have CKB
Blake2b-256 identities.

The generated profile records `script_role = "lock"`,
`encoding = "linear-le-v0"`, `build.reproducible = false`, and
`security.status = "review_required"`. The command does not manufacture a
reproducibility or audit claim.

## 5. Dry-Run and Publish

Validate the generated bundle locally before any Registry write:

```bash
cellc publish \
  --artifact-manifest Artifact.toml \
  --dry-run \
  --json
```

The dry-run rechecks the coordinate, object roles, size limits, profile
contract, raw IDL schema and hashes, and executable suffix. It does not upload
anything.

For a first production publish, let `cellc` create a scoped delegated key and
open the wallet authorisation flow:

```bash
cellc publish \
  --artifact-manifest Artifact.toml \
  --authorise \
  --json
```

`cellc` prints the pending `cap_...` publishing key ID, stores its private key
in the local OS keychain, opens a 15-minute browser session, waits for CKB
wallet approval, and then continues the publish. The private key does not enter
the browser.

This short interactive path requests only
`publish:example/demo-lock`. It deliberately does not grant permission to
attach deployment evidence or change availability. Those scopes are
independent.

For the isolated Pudge Testnet Registry, select its API explicitly:

```bash
cellc publish \
  --artifact-manifest Artifact.toml \
  --authorise \
  --api-url https://api.testnet.registry.cellscript.dev \
  --json
```

Production and Testnet have separate API origins, databases, object stores,
wallet state, and chain evidence. Do not publish to one environment and record
the deployment in the other.

After admission, the release starts with independent states similar to:

```text
verification_status = pending
deployment_status   = undeployed
availability_status = active
```

The Registry worker must accept the immutable bundle and promote its integrity
evidence before deployment evidence can be attached. A `hash_bound` result is
an identity/integrity statement, not a security review.

## 6. Deploy the Bound Artifact and Record Evidence

Deploy `build/demo-lock.ls-idl`, not `build/demo-lock`. Deployment transaction
construction, capacity, fees, witnesses, signing, and broadcast remain in the
external CKB builder and wallet.

After the deployment transaction is committed, attach its OutPoint to the
published release. First authorise a delegated key with the exact deployment
scope. The `principal_id` is the normalized identity binding derived from the
connected signer, not the displayed CKB address. Choose `joyid_ckb` or
`ckb_secp256k1` for `--principal-type`:

```bash
cellc auth capability create \
  --principal-type <principal-type> \
  --principal-id <normalized-wallet-principal-id> \
  --scope deployment:example/demo-lock \
  --expires 90d \
  --json > deployment-capability.json
```

Sign the payload with the matching CKB wallet, save the wallet result as
`deployment-wallet-signature.json`, and submit it:

```bash
cellc auth capability submit \
  --payload deployment-capability.json \
  --wallet-signature deployment-wallet-signature.json \
  --json
```

`create` stores the generated delegated private key in the local OS keychain;
`submit` returns its `cap_...` key ID after the Registry accepts the
wallet-rooted grant. For Testnet, add
`--registry-origin https://api.testnet.registry.cellscript.dev` to `create`
and `--api-url https://api.testnet.registry.cellscript.dev` to `submit`.

The manual flow may request `publish:` and `deployment:` together when one key
must perform both operations, but neither scope implies the other. This
tutorial keeps them separate so a deployment key cannot publish a new release.

Then record the Testnet deployment:

```bash
cellc artifact record-deployment example/demo-lock@0.1.0 \
  --network testnet \
  --api-url https://api.testnet.registry.cellscript.dev \
  --code-hash 0x<64-hex> \
  --hash-type data1 \
  --dep-type code \
  --tx-hash 0x<64-hex-deployment-transaction-hash> \
  --index 0 \
  --capability-key-id cap_<deployment-key-id> \
  --json
```

For production, use `--network mainnet` and the default production Registry
origin. The command signs the deployment record with the delegated key, while
the Registry verifies the configured RPC network, live code Cell, committed
creation transaction, confirmations, artifact data hash, Script identity, and
declared deployment mode.

The immutable profile and the evidence command must agree on `hash_type` and
`dep_type`:

- for `data`, `data1`, or `data2`, `code_hash` identifies the bound code Cell
  data; and
- for `type`, `code_hash` is the code Cell Type Script hash. Later LS-IDL
  lookup also needs the current code Cell `data_hash` to disambiguate the
  executable bytes.

Do not describe a locally generated deployment payload as chain evidence. The
Registry lookup becomes available only after the release is active, public,
and backed by accepted chain-verified deployment evidence.

## 7. Fetch the Exact IDL by Script Identity

For a mainnet `data1` Script:

```bash
cellc artifact ls-idl fetch \
  --code-hash 0x<64-hex> \
  --hash-type data1 \
  --network mainnet \
  --output fetched-idl.json \
  --json
```

For Testnet, use the matching Registry API:

```bash
cellc artifact ls-idl fetch \
  --code-hash 0x<64-hex> \
  --hash-type data1 \
  --network testnet \
  --api-url https://api.testnet.registry.cellscript.dev \
  --output fetched-idl.json \
  --json
```

For a Type Hash deployment, add the live code Cell data hash:

```bash
cellc artifact ls-idl fetch \
  --code-hash 0x<64-hex-type-script-hash> \
  --hash-type type \
  --data-hash 0x<64-hex-code-cell-data-hash> \
  --network mainnet \
  --output fetched-idl.json \
  --json
```

`fetch` refuses to overwrite an existing output unless `--force` is explicit.
Before writing, it requires the LS-IDL content type and
`schema-and-suffix-bound` verification header, enforces the 256 KiB limit,
validates the LS-IDL schema, hashes the response body directly, and compares
that digest with the Registry header.

The public browser flows expose the same lookup for
[Mainnet](https://cellscript.dev/registry/LS-IDL) and the isolated
[Pudge Testnet](https://testnet.registry.cellscript.dev/registry/LS-IDL/).
Lookup and download are read-only; connecting a wallet is needed only for
Registry writes.

## 8. Understand the Evidence Boundary

The workflow deliberately separates evidence:

| Stage | What it establishes |
|---|---|
| `validate --idl` | supported schema and SHA-256 of the exact IDL bytes |
| `validate --executable` | the executable ends with that exact 32-byte digest |
| `bundle` | source, executable, ABI objects, hashes, and closed profile agree |
| Registry verification | admitted immutable objects satisfy the same profile contract |
| `record-deployment` | accepted live-chain deployment evidence matches the published artifact |
| `fetch` | the returned exact bytes match the Registry digest and verification contract |

None of these stages proves that the Lock Script actually decodes every field
as described, applies the intended signing rules, authorises the correct user,
accepts a complete valid transaction set, rejects every invalid transaction,
or is secure. LS-IDL Registry support is a byte-identity and deployment-binding
contract. Implementation tests, CKB-VM transaction tests, review, and audit
remain separate responsibilities.

`cellc verify-artifact` is also a different boundary. It independently checks
the four-file lowering/source-map bundle emitted by CellScript CKB builds; it
is not a replacement LS-IDL verifier for an arbitrary Rust or C executable.

### The executable suffix does not match

The IDL was changed or reformatted after binding, or the unbound executable was
selected. Return to the clean executable and bind the final IDL bytes again.

### `cellc` refuses to overwrite a file

This is intentional. Use a new output path, or pass `--force` only after
checking the exact target.

### Type Hash lookup requires `--data-hash`

A Type Script hash may identify more than one executable data revision. Supply
the current code Cell data hash; the Registry returns `409` rather than choosing
an ambiguous deployment.

### Fetch returns not found

Check the Registry environment, network, `code_hash`, `hash_type`, and optional
`data_hash`. A published bundle is not enough: the release must also be active,
public, and chain-verified on the requested network.

### Publish succeeds but lookup is not ready

Publication, verification, and deployment are separate states. Wait for bundle
verification, deploy the bound artifact, and record the committed deployment
OutPoint before expecting Script-identity lookup to succeed.

## Next

Read [Registry Artifacts End to End](Tutorial-12-Phase1-Registry-End-to-End.md)
for publisher capabilities, deployment evidence, availability, and artifact
consumption. Use the
[LS-IDL Registry Profile](../CELLSCRIPT_LS_IDL_REGISTRY_PROFILE.md) as the
closed schema and API reference.
