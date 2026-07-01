---
name: cellscript-metadata-audit
description: CompileMetadata, ProofPlan, builder assumptions, constraints, ABI, audit bundles, receipts, and artifact verification.
references:
  - docs/wiki/Tutorial-06-Metadata-Verification-and-Production-Gates.md
  - docs/wiki/Tutorial-11-Scoped-Invariants-and-ProofPlan.md
  - docs/CELLSCRIPT_GATE_POLICY.md
commands:
  - cellc metadata
  - cellc constraints
  - cellc explain proof
  - cellc audit-bundle
  - cellc verify-artifact
  - cellc verify-receipt
---

# CellScript Metadata Audit

Use this skill when reviewing compiler evidence. Treat metadata as an audit
stream, not consensus truth. ProofPlan rows, TemplateLayout records, receipts,
constraints, ABI, and builder assumptions explain what the compiler emitted and
what remains to be checked by builders or CKB nodes.

Distinguish evidence states precisely: compile-only, metadata-only,
runtime-required, helper-backed, builder-backed, node dry-run, tx-pool accepted,
submitted, and externally attested.

Validation defaults:

- run `cellc metadata . --target-profile ckb` to inspect metadata without
  writing a file;
- run `cellc explain proof . --target-profile ckb --json` for ProofPlan;
- run `cellc verify-artifact` before trusting artifact/metadata identity.
