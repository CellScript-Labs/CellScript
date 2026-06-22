# CellScript Metadata System Audit

**Scope:** `src/lib.rs`, `src/cli/commands.rs`, `tests/cli.rs`
**Date:** 2026-05-22
**Last updated:** 2026-06-22
**Review status:** Corrected after local validation and hardening. The disk-loaded artifact path now validates artifact byte length and `constraints.artifact` format/size in addition to the artifact hash. The metadata envelope now also carries split source/package, artifact, and constraints component schema versions.

---

## 1. Metadata Schema Version

**Finding: strict exact-match enforcement with component versions.**

`METADATA_SCHEMA_VERSION` remains the top-level envelope version, and `validate_compile_metadata()` rejects any metadata whose `metadata_schema_version` differs from the current compiler constant. The metadata record also carries:

- `source_metadata_schema_version`
- `artifact_metadata_schema_version`
- `constraints_metadata_schema_version`

Each component version is checked against its own compiler constant. This lets future schema work distinguish source/package identity changes from artifact-binding changes and CKB constraint-summary changes without pretending they all have the same compatibility risk.

This is a compatibility wall:

- newer metadata is rejected;
- older metadata is rejected;
- metadata with a mismatched source, artifact, or constraints component version is rejected;
- the compiler version is also checked for exact equality;
- there is no declared `min_schema_version`, `max_schema_version`, semver range, or downgrade loader.

One important nuance: JSON decoding is not shape-strict. The metadata structs do not use `#[serde(deny_unknown_fields)]`, so unknown JSON fields are ignored by Serde by default. Several fields also use `#[serde(default)]`, which allows omitted fields to decode with defaults. The component schema fields default to `0` when omitted so older files can be parsed far enough to produce an explicit version rejection. The exact schema-version wall still prevents cross-version loading when the version changes, but same-version metadata is not a byte-for-byte or field-exhaustive schema check.

**Verdict:** Pass for incompatible-version rejection. Do not describe the loader as rejecting every unknown field.

---

## 2. Artifact Hash, Size, and VM ABI Trailer

**Finding: hash binds to the on-disk artifact byte sequence; size is now validated on both in-memory and disk-loaded paths.**

The compile path appends the VM ABI trailer before hashing when `metadata.runtime.vm_abi.embedded_in_artifact` is true. The artifact hash is computed over the exact stored bytes.

Validation now checks:

- `artifact_hash` matches `artifact_bytes`;
- `metadata.artifact_hash` matches the computed hash;
- `metadata.artifact_size_bytes` matches `artifact_bytes.len()`;
- `metadata.constraints.artifact.format` matches the declared artifact format;
- `metadata.constraints.artifact.artifact_size_bytes` matches `artifact_bytes.len()`.

For ELF artifacts, validation also checks trailer consistency:

- ELF magic is present;
- an embedded trailer is rejected when metadata says the profile must not embed one;
- a missing trailer is rejected when metadata says one is required;
- a present trailer must match `metadata.runtime.vm_abi.version`;
- stripping the trailer must still leave ELF magic.

Current CKB behavior is simpler: `TargetProfile::Ckb.embeds_vm_abi_trailer()` returns `false`, so CKB artifacts do not embed trailer bytes. `strip_vm_abi_trailer()` remains a runtime-loader concern for paths such as `cellc run`.

**Verdict:** Pass. The hash covers stored bytes, and the validation layer now binds the sidecar size fields to those bytes as well.

---

## 3. CKB Constraint Fields

**Finding: on-chain measurables are explicitly marked as requiring builder evidence.**

The compiler computes static lower bounds and metadata summaries, but it does not claim to measure production transaction properties.

| Field | Compiler Behavior |
| --- | --- |
| `estimated_cycles` | Derived from action metadata where available. |
| `measured_cycles` | Always `None`. |
| `cycles_status` | `"not-measured-by-compiler"`. |
| `tx_size_bytes` | Always `None`. |
| `tx_size_measurement_required` | Always `true`. |
| `tx_size_status` | `"builder-required"`. |
| `occupied_capacity_measurement_required` | Derived from created or mutated outputs. |
| `capacity_status` | Builder-required when output planning is needed; otherwise code-cell lower bound only. |
| `dry_run_required_for_production` | Always `true`. |
| `capacity_evidence_contract.required` | Always `true`. |
| `capacity_evidence_contract.measured_*` | Always `None`. |

`validate_ckb_constraints_summary_metadata()` recomputes the summary fields that are derivable from metadata, including created and mutated output counts, timelock/runtime feature flags, and capacity evidence requirement booleans.

**Verdict:** Pass. The compiler is explicit that cycles, transaction size, and occupied capacity require builder or dry-run evidence.

---

## 4. Molecule Schema Manifest

**Finding: the manifest is validated for metadata-internal consistency, not independently regenerated from source, IR, or artifact bytes.**

Generation builds a canonical manifest from `TypeMetadata` entries that contain Molecule schema metadata, including:

- type name;
- kind;
- layout;
- fixed or dynamic size markers;
- dynamic fields;
- schema hash;
- field offsets.

Validation checks:

- schema name, version, ABI, and target profile;
- type counts and fixed/dynamic counts;
- sorted entries;
- each manifest entry against the corresponding `TypeMetadata` and `MoleculeSchemaMetadata`;
- per-type schema hashes against the serialized schema text;
- manifest hash against a recomputed canonical manifest built from `metadata.types`.

This prevents drift between manifest entries and type metadata. It does not prove that the sidecar metadata as a whole came from the artifact, source, or IR. A party that can rewrite the entire metadata file consistently can also rewrite type metadata, schema text, schema hashes, manifest entries, and the manifest hash unless an external signature or trusted artifact provenance is used.

**Verdict:** Pass for internal metadata consistency. Do not describe the manifest as an independent cryptographic binding to source, IR, or artifact bytes.

---

## 5. Backward and Forward Compatibility

**Finding: no compatibility contract is declared.**

The metadata model has no compatibility range fields and no reduced-feature loader for older metadata. `CompileOptions.primitive_compat` is a source-language frontend compatibility mode; it is unrelated to artifact metadata loading.

The `LoweringMetadata.semantics_preserving_claim` string is a disclaimer, not a compatibility guarantee. It explicitly states that pure computation lowering is executable while stateful protocol lowering is represented in metadata and assembly but is not yet a proved schema decoder/verifier.

Unknown JSON fields may be ignored for the current schema version because Serde defaults to permissive unknown-field handling. That behavior is not a documented forward-compatibility contract and should not be relied on for cross-version metadata interchange.

**Verdict:** Pass by absence of false compatibility claims. The loader is version-strict but not field-exhaustive within the same version.

---

## 6. Sidecar Trust Boundary

**Finding: metadata is validated aggressively but is not authenticated.**

The sidecar metadata is plain JSON. Validation binds the artifact hash and size to artifact bytes, and it validates many metadata-internal summaries by recomputation. It does not sign or MAC the sidecar, and it does not recompute proof-plan or Molecule metadata from source/IR when verifying an already-built artifact.

This means:

- trusted local compiler output is safe to consume as a sidecar contract;
- untrusted sidecar metadata must be treated as advisory unless accompanied by an external signature, trusted distribution channel, or source/build reproducibility check;
- `verify-artifact --verify-sources` improves source provenance checks, but it is not a cryptographic signature scheme for the full metadata document.

**Verdict:** Medium trust-model risk when metadata crosses a trust boundary. Not a critical issue for local compiler-produced artifacts.

---

## Summary

| Question | Verdict |
| --- | --- |
| `metadata_schema_version` enforcement | Pass: strict envelope exact match, strict component exact matches, plus compiler-version exact match. |
| Unknown JSON fields | Nuance: ignored by default unless `deny_unknown_fields` is added. |
| Artifact hash and size binding | Pass: hash and size are validated on compile-result and disk-loaded paths. |
| VM ABI trailer handling | Pass: hash covers stored bytes; runtime strips only before VM loading. |
| CKB constraints | Pass: measured on-chain values are explicitly builder-required. |
| Molecule manifest | Pass for metadata-internal consistency; not an independent source/IR/artifact proof. |
| Compatibility claims | Pass: no false compatibility range is declared. |
| Sidecar authentication | Medium trust-boundary gap: no signature or MAC. |

No critical issues remain under the trusted local-build workflow. The main remaining hardening option is an authenticated metadata envelope for sidecars shared across trust boundaries.
