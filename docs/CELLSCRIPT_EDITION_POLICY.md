# CellScript Edition Policy

Edition 2027 is the only source contract. Every package declares it explicitly:

```toml
[package]
edition = "2027"
```

The compiler records `cellscript-source-semantics-2027-0.33-effect-facts-v1` and
infers effects from operations and resolved callees. Source effect declarations
are rejected. The [source contract](CELLSCRIPT_SOURCE_CONTRACT.md) defines the
current artifact/interface generations and consumer boundary.

## What The Edition Owns

An edition owns rules that can change the meaning of the same source text:

- keywords, reserved words, and resolution of syntactic ambiguities;
- name resolution, scope behavior, and the default prelude;
- type checking, inference defaults, coercions, and flow/resource rules;
- desugaring and other source-observable semantics; and
- edition-specific deprecation diagnostics and migration lints.

Absolute transaction locations and current-group locations remain distinct.
Preview4 admits one final native `type_script` or `lock_script` container, not
both in one module. Its bounded Type Script surface distinguishes successor,
fixed-role pooled accounting, retirement, and fresh output origin; `audit` is
metadata-only. These restrictions do not apply to ordinary modules. Existing
terminal `consume`/`consume_each` operations retain their semantic records and
are not reinterpreted as native retirement or pooled accounting. Default
parameters retain type-directed provenance; provenance is not authorization.

Additive syntax, diagnostics, formatter improvements, and optimizer changes do
not require a new edition when existing source keeps its meaning. A new edition
is justified only when an intentional source-semantic break cannot be handled
by an additive feature, a warning/deprecation cycle, or an independently
versioned schema or ABI.

## Independent Compatibility Axes

The edition does **not** own the compiler release, target profile,
primitive-assurance mode, metadata schemas, or CKB wire ABIs. The compiler
assembles those independently versioned values with the source edition into a
resolved compatibility profile:

| Axis | Current implementation value |
|---|---|
| Source edition | only `2027` |
| Source semantics | `cellscript-source-semantics-2027-0.33-effect-facts-v1` |
| Compiler release | workspace SemVer (`0.x.y`), recorded separately |
| Target profile | selected independently, normally `ckb` |
| Primitive assurance | selected independently, or `default` |
| Payload ABI | Single entry: `cellscript-entry-witness-v1` (`CSARGv1\0`); explicit Type policy: `cellscript-policy-witness-v1` (`CSPOLv1\0`) |
| Placement ABI | Single entry: `cellscript-witnessargs-input-type-v2`; explicit Type policy: `cellscript-policy-witnessargs-input-type-v1` |
| Metadata schemas | metadata 75, source 2, artifact 1, constraints 4 |

The compiler release is recorded next to the profile but is not part of the
profile itself. A compiler patch may change diagnostics or optimization
without changing compatibility. Conversely, an urgent wire-ABI or metadata
fix can advance its own version immediately without waiting for a new calendar
year or source edition.

Policy selection is explicit and changes the entry compatibility profile, not
the source edition or inner CSARG argument bytes. The
[policy witness ABI](CELLSCRIPT_POLICY_WITNESS_ABI.md) defines its bounded
multi-record ownership and exact current Script-hash selection. Unselected
builds retain the single-entry profile even if a package declares policies.

For the current single-entry CKB placement profile:

| Contract | Value |
|---|---|
| Placement field | `WitnessArgs.input_type` |
| Witness source | `GroupInput#0`, then `GroupOutput#0` |
| Raw payload alias | rejected |

The resolved profile uses schema
`cellscript-resolved-compatibility-profile-v1`. It is emitted in compile
metadata and hashed into package, registry, lockfile, deployment, receipt, and
generated-builder identities. Changing any constituent axis changes the
profile identity even when source text and edition stay the same.

```mermaid
flowchart LR
    E["Source edition<br/>2027"] --> R["ResolvedCompatibilityProfile"]
    T["target profile"] --> R
    P["primitive assurance"] --> R
    W["entry + placement ABI"] --> R
    S["metadata schemas"] --> R
    R --> M["compile metadata + profile hash"]
    M --> I["registry + Cell.lock + Deployed.toml"]
    M --> B["receipt + generated builder"]
```

Registry records therefore retain both `edition` and
`compatibility_profile_hash`. The former tells source consumers how to read the
package; the latter commits to the complete compile/build contract. A registry
consumer must not infer target, primitive, ABI, or metadata versions from the
edition year.

## Why `CSARGv1` Still Exists

The source edition and `CSARGv1` solve different problems.

- `edition = "2027"` selects source-language meaning before a transaction
  exists.
- `CSARGv1\0` identifies CellScript positional-argument bytes while a CKB
  Script is executing.
- `cellscript-witnessargs-input-type-v2` identifies where those bytes are
  placed and how the script-group witness is selected.

The current compatibility profile combines all three identities. The old raw
placement form—putting `CSARGv1` directly in the witness instead of inside a
canonical `WitnessArgs.input_type`—is not accepted. It fails closed with
runtime error `25 entry-witness-abi-invalid` because the placement ABI says so,
not because an edition year intrinsically implies a witness layout.

## Persisted Format Boundary

The 0.23 line deliberately starts new persisted identities:

| Surface | Required identity |
|---|---|
| Compile metadata | metadata 57, source 2, artifact 1, constraints 2 |
| Compatibility profile | `cellscript-resolved-compatibility-profile-v1` with every independent axis |
| `Cell.lock` | version 2 |
| `Deployed.toml` | version 2 and `cellscript-deployed-v2` |
| Compile receipt | `cellscript-compile-receipt-v3` |
| Generated action builder | `cellscript-generated-action-builder-v1` |

Readers reject earlier versions. They do not silently fill edition/profile
fields or rewrite old files.

The alpha.2 fact-protocol cutover supersedes the affected rows above: metadata
74 (source 2 / artifact 1 / constraints 4), compile receipt v4, package interface
v5, and the current edition-specific `0.33-effect-facts-v1` source identities
are required. Old effect/interface/receipt readers are removed; rebuild the
complete source/bundle instead of translating it. The complete current identity
table is in [the effect contract](CELLSCRIPT_EFFECT_CONTRACTS.md). Historical
format allocations above do not describe an accepted reader or compatibility mode.

The 0.24 line advanced the lock carrier to version 3. The 0.30 development
line advances it to version 5 with schema
`cellscript-lock-v0.30-single-package-coordinate-v1`, binding the root and every
dependency's compiler range plus the compiler release that resolved the graph.
It also declares the `single-package-coordinate-v1` resolver model.
This is a dependency-resolution and source-tool compatibility change, not a
new source edition: the source edition, the compatibility profile, `Deployed.toml`,
receipt, and generated-builder identities remain independently versioned.
Build/check/test reject older locks; only explicit `cellc lock` or
`cellc update-plan` may propose a repin and `cellc update --apply-plan` may
apply the reviewed receipt.

## API Boundary

Package compilation reads the mandatory edition from `Cell.toml`. APIs without
a package manifest must receive the edition explicitly:

- native metadata-only Rust APIs take `CellScriptEdition`;
- WASM exports take an edition string and accept only `"2027"`;
- browser workers pass `"2027"` explicitly; and
- LSP package compilation resolves the nearest package manifest.

`CompileOptions::default()` uses the current edition only for in-memory and
standalone compiler use. It is not a fallback for a package missing `edition`.

## Release And Evolution Requirements

Different axes have different closure requirements:

- source-semantic changes require a new edition plus parser, formatter, type
  checking, lowering, metadata, LSP, migration diagnostics, docs, and tests;
- entry or placement ABI changes require a new ABI identity plus codegen,
  builder, metadata, CKB-VM, `backend`, `dev`, and `ci` evidence;
- metadata changes require a schema bump plus every reader/validator update;
- target and primitive changes retain their own profile and gate contracts; and
- compiler-only compatible improvements use ordinary SemVer releases.

No edition is created merely because a year or compiler release changed.

## Source editions and retired wire generations

Implicit entry selection and explicit artifact policies describe different
execution/dispatch contracts. `IrEntrySelection::Implicit` names the current default selection directly.
The enum is not serialized into metadata, interfaces or witness protocols;
producer/consumer matches and entry-selection tests were audited for the rename.
There is no alias for the retired Rust variant name and no old wire reader.

Stack collections, arena-owned collections and borrowed/nested views have
different ownership, storage, bounds and invalidation evidence. They remain
separate models for those reasons. A decoded nested slice grants no transaction
or lifecycle authority; local slice inspection must not invent a transaction
read. These distinctions are independent of source editions and wire versions.
