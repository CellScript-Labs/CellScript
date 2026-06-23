# DOB-EVO/1 Evolving DOB Profile

**Status**: production-ready source package. It uses the NovaSeal package tree
shape, but the protocol is DOB-specific and does not depend on NovaSeal.

DOB-EVO/1 implements dynamic DOB state as a typed CKB state line:

```text
immutable Spore DOB + live DobEvolutionStateV1 + decoder = current render state
```

It deliberately does not mutate the public Spore DOB Cell and does not support
older evolving-DOB encodings.

## Contents

| Path | Purpose |
| --- | --- |
| `src/evolving_dob_type.cell` | Production CellScript state, event, and action surface. |
| `schemas/` | Source-of-truth state, intent, and event layout notes. |
| `fixtures/` | Positive and negative fixture labels for builder and CKB VM harnesses. |
| `proofs/` | Invariant matrix and ProofPlan mapping. |
| `docs/` | Profile, security, production gate, and registry pressure notes. |
| `scripts/evolving_dob_registry_pressure.py` | Local package/registry pressure gate. |
| `scripts/evolving_dob_devnet_workflow.py` | Local CKB devnet deployment, live registry, action-plan, and builder gate. |

## Required Local Gate

```bash
cd proposals/evolving-dob/evolving-dob-profile-v1
cellc build --release --target riscv64-elf --target-profile ckb
cellc check --target-profile ckb --primitive-strict 0.16
cellc package verify --json
cellc publish --dry-run
python3 scripts/evolving_dob_registry_pressure.py
```

The strict promotion probe must pass without `PP0150`, fail-closed verifier
features, or runtime-required ProofPlan obligations.

## Strict Local Devnet Workflow

With a CKB checkout or `CKB_BIN` available, run:

```bash
python3 scripts/evolving_dob_devnet_workflow.py --pretty
```

This starts a local integration node, deploys `build/evolving_dob_type.elf` as
a live code cell, writes `Deployed.toml`, bridges the deployment into
`Cell.lock`, runs strict registry verification including `--live`, emits all
three action build plans, generates the TypeScript builder with deployment
identity, and runs the generated builder's `npm test` suite. The local workflow
does not claim a cryptographic publisher signature; that is reserved for public
registry promotion.

This is real local-node workflow evidence. It is not a claim that the package
has been deployed to public Aggron/mainnet infrastructure.
