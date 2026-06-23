# Registry Pressure Target

This package is intended to exercise the CellScript package system in a
production-like profile:

- namespace-aware package identity: `dob/evolving-dob-profile-v1`;
- source-root hashing from `Cell.toml` plus `src/**/*.cell`;
- `cellc publish --dry-run` registry validation;
- build-lock identity in `Cell.lock`;
- strict `PP0150` promotion acceptance with no runtime-required ProofPlan gaps;
- fail-closed deployment verification once `Deployed.toml` exists;
- no compatibility branches that would hide registry/source drift.

The pressure script performs local checks that do not require a live CKB node.
The script also checks the local `registry.json` package identity when the
registry file has been generated.

Live workflow pressure is handled separately by
`scripts/evolving_dob_devnet_workflow.py`. That gate starts a local CKB
integration node, deploys the actual compiled ELF, records the resulting
`Deployed.toml`, runs `registry verify --live`, generates the deployment-bound
TypeScript builder, and runs the generated builder tests. This keeps the
offline package gate fast while still requiring concrete node evidence before
any devnet workflow is accepted.
