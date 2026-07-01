---
name: cellscript-builder-deployment
description: Generated builders, action-aware scans, deployment plans, live registry verification, and evidence boundaries.
references:
  - docs/CELLSCRIPT_CKB_ADAPTER.md
  - docs/CELLSCRIPT_CAPACITY_AND_BUILDER_CONTRACT.md
  - docs/CELLSCRIPT_PACKAGE_PROVENANCE_AND_DEPLOYMENT_IDENTITY.md
  - examples/ckb-sdk-builder/README.md
commands:
  - cellc action build
  - cellc gen-builder
  - cellc deploy plan
  - cellc deploy verify
  - cellc tx validate
---

# CellScript Builder And Deployment

Use this skill for builder and deployment work. The compiler emits semantic
plans and metadata. Builders provide concrete live Cells, output data, CellDeps,
witnesses, capacity/fee evidence, dry-run evidence, signing, and optional
submission.

Do not claim CKB production readiness from compile-only evidence. A plain
`ActionPlan` is not a submitted transaction. A `ResolvedActionTx` is adapter
materialisation. `AcceptedActionTx` requires node-facing evidence.

Validation defaults:

- run `cellc action build --json` for action plan shape;
- inspect `action_scan_selectors` / `actionScanSelectors` for compile-only
  live-cell scan guidance derived from `transaction_runtime_input_requirements`;
- require runtime adapters to return `scanSelectorEvidence` for generated
  `actionScanSelectors`; missing or mismatched selector evidence is a
  pre-transaction builder failure, not a CKB acceptance claim;
- use `transaction_draft.scan_selector_evidence` for the equivalent
  materialised `ActionPlan` JSON consumed by the Rust adapter;
- run `cellc deploy plan --json` for deployment planning;
- run `cellc tx validate --json` against concrete transaction evidence.
