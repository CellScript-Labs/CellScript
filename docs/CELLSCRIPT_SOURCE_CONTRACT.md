# CellScript Source Contract

Edition 2027 is the sole source-language contract. Every package explicitly
sets `edition = "2027"`; unknown or missing values are errors. Standalone native
and language-service defaults use the same contract, and browser callers pass
it explicitly.

Ordinary `resource`, `action`, `lock`, `require`, explicit or omitted
`verification`, value types and lifecycle operations share one checked syntax
kernel. Native `type_script` and `lock_script` containers have separately bounded
surface rules. The grammar and runtime admission boundaries remain evolving.

## Artifact and interface contract

Current bundles use metadata 75, typed semantics and lowering records v12,
semantic foundation v4 and package interface v6. Operation records describe
actual input/output disposition and its present policy boundary. Public items
with omitted visibility use `implicit-public`; public value layouts require
explicit fixed, serializable, non-linear parameters.

Deployment records use `cellscript-deployed-v2` with numeric version 2.
Generated action builders use `cellscript-generated-action-builder-v1`.
Their exact schema identities, the source edition, compiler release, target,
assurance policy, witness payload and placement are independent contracts.
Readers admit one current shape; producers and consumers must agree on it.

## Packages and tools

`Cell.lock` binds the reviewed manifest/source graph. Only explicit `cellc lock`
or dependency update operations select versions and write pins. Build/check/test
validate the existing graph; `--frozen` additionally forbids network and writes.

Native metadata, LSP workspace analysis and hover share manifest-aware inputs.
An invalid package cannot produce successful semantic hover metadata through a
standalone compiler fallback. WASM, Registry, generated builders, adapters and
project examples use the same current interface and record vocabulary.

## Verification

Behavioral tests cover explicit and concise spellings, field preservation,
resource disposition, source/Cell bindings, runtime positives and negatives,
unknown contracts, hash-rebound artifact mutations, package locks and consumer
builds. Fixtures are current compiler output, independently checked where the
artifact boundary applies. The `commit`, `dev`, `ci` and `backend` gates keep
their documented scopes; none imply publication or production acceptance.
