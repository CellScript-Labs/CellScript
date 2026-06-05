# NovaSeal Full Devnet Acceptance Runbook

Single-source document for re-running the complete NovaSeal V1 devnet
acceptance from a clean checkout. Every profile doc links back here.

---

## 1. Prerequisites

| Tool | Version / Commit | Purpose | Install |
|---|---|---|---|
| CKB node | `develop` branch, built from `nervosnetwork/ckb` | Devnet chain for all CKB profiles | `cargo build --release` in ckb checkout |
| ckb-cli | `develop` branch, commit `a3450f91` | Fiber dev-chain setup helper | `cargo build --release` in ckb-cli checkout |
| Fiber node | `develop` branch, commit `27d458b8529e` | 16-suite Fiber e2e | `git clone https://github.com/nervosnetwork/fiber.git` at sibling dir |
| LND | `v0.20.1-beta`, built with `invoicesrpc routerrpc` | Cross-chain hub suites | `go install -tags=\"invoicesrpc routerrpc\"` in lnd checkout |
| Bruno CLI | `@usebruno/cli` via npm | Fiber e2e runner | `npm install` inside fiber/tests/bruno |
| Python | 3.10+ | All scripts | System python3 |
| Rust toolchain | stable + nightly for clippy | Build cellscript | rustup |
| Go | 1.22+ | Build LND | System go |

**PATH requirements:** `ckb/target/debug` (or `release`), `ckb-cli/target/debug`,
and Go bin dir must all be on `$PATH` before running scripts.

**Ports:** Scripts auto-pick free ports. No manual configuration needed unless
you use `--keep-node` for debugging.

---

## 2. Freshness Rule

The Rust certification reducer (`src/cli/novaseal_certification.rs`) enforces
**content-addressed provenance**, not git-commit matching. A report is fresh
only when:

1. The SHA-256 of the tracked source files (.cell, .schema, .toml, .py, .rs)
   matches the `source_tree.sha256` recorded in the report.
2. The SHA-256 of each tracked artifact (verifier ELF, lifecycle ELF) matches
   the `artifacts.*.sha256` recorded in the report.

This means: if you change any tracked source file after generating a report,
that report becomes stale and certification will fail. You must re-run the
affected script. Changing untracked files (README, docs not in the source path)
does **not** break freshness.

**Do not** copy or cherry-pick JSON reports from older checkouts. Generate
them fresh.

---

## 3. Command Sequence

### Phase 1: Build CellScript

```bash
cargo build --locked -p cellscript --all-targets
```

### Phase 2: Core Live Devnet

```bash
python3 scripts/novaseal_devnet_stateful_live.py \
  --ckb-repo /path/to/ckb \
  --ckb-bin /path/to/ckb/target/debug/ckb \
  --pretty
```

Output: `target/novaseal-devnet-stateful-live.json`

Doc: `proposals/novaseal/v0-mvp-skeleton/docs/DEVNET_STATEFUL_ACCEPTANCE.md`

### Phase 3: Agreement Live Devnet

```bash
python3 scripts/novaseal_agreement_devnet_stateful_live.py \
  --ckb-repo /path/to/ckb \
  --ckb-bin /path/to/ckb/target/debug/ckb \
  --pretty
```

Output: `target/novaseal-agreement-devnet-stateful-live.json`

Doc: `proposals/novaseal/agreement-profile-v0/docs/DEVNET_STATEFUL_ACCEPTANCE.md`

### Phase 4: Planned Profile Live Devnet (6 profiles)

One command per profile. Each starts its own CKB devnet node.

```bash
for profile in fungible-xudt rwa-receipt btc-transaction-commitment \
               btc-utxo-seal dual-seal fiber-candidate; do
  python3 scripts/novaseal_planned_profiles_devnet_stateful_live.py \
    --ckb-repo /path/to/ckb \
    --ckb-bin /path/to/ckb/target/debug/ckb \
    --profile "$profile" \
    --pretty
done
```

Outputs:
- `target/novaseal-fungible-xudt-devnet-stateful-live.json`
- `target/novaseal-rwa-receipt-devnet-stateful-live.json`
- `target/novaseal-btc-transaction-commitment-devnet-stateful-live.json`
- `target/novaseal-btc-utxo-seal-devnet-stateful-live.json`
- `target/novaseal-dual-seal-devnet-stateful-live.json`
- `target/novaseal-fiber-candidate-devnet-stateful-live.json`

Docs: each `proposals/novaseal/<profile>/docs/DEVNET_STATEFUL_ACCEPTANCE.md`

### Phase 5: Fiber Node Experiments (16 suites)

**This is the longest phase.** Each suite starts a local CKB dev chain, builds
or reuses Fiber `fnn`, starts three Fiber nodes, then runs the Bruno e2e
suite. Budget 20-40 minutes per suite, ~6 hours total.

```bash
FIBER_REPO=/path/to/fiber
CKB_PATH=/path/to/ckb/target/debug
CKB_CLI_PATH=/path/to/ckb-cli/target/debug
GO_PATH=/path/to/go/bin
export PATH="$GO_PATH:$CKB_PATH:$CKB_CLI_PATH:$PATH"

for suite in invoice-ops open-use-close-a-channel 3-nodes-transfer \
             router-pay shutdown-force reestablish external-funding-open \
             funding-tx-verification udt udt-router-pay \
             watchtower/force-close-after-open-channel \
             watchtower/force-close-with-pending-tlcs \
             watchtower/force-close-with-pending-tlcs-and-udt \
             watchtower/force-close-preimage-multiple; do
  REMOVE_OLD_STATE=y python3 scripts/novaseal_fiber_node_experiments.py \
    --fiber-repo "$FIBER_REPO" \
    --run-suite "$suite" \
    --timeout-seconds 1800 \
    --pretty
done

# Cross-chain hub suites need LND (longer timeout)
for suite in cross-chain-hub cross-chain-hub-separate; do
  REMOVE_OLD_STATE=y python3 scripts/novaseal_fiber_node_experiments.py \
    --fiber-repo "$FIBER_REPO" \
    --run-suite "$suite" \
    --timeout-seconds 2400 \
    --pretty
done
```

Output: `target/novaseal-fiber-node-experiments.json`

Doc: `proposals/novaseal/fiber-candidate-profile-v0/docs/FIBER_NODE_EXPERIMENTS.md`

**Known issue:** The cross-chain suites require LND built with
`invoicesrpc routerrpc`. Without those tags, `AddHoldInvoice` fails with
`unknown service invoicesrpc.Invoices`. Build LND as:

```bash
cd /path/to/lnd && git checkout v0.20.1-beta
go install -tags="invoicesrpc routerrpc" ./cmd/lnd ./cmd/lncli
```

### Phase 6: Fixture and Report Generation

These scripts are pure computation (no external services). Run in order:

```bash
# BIP340 TCB review
python3 scripts/novaseal_bip340_tcb_review.py --pretty

# Wallet signing vectors
python3 scripts/novaseal_wallet_signing_vectors.py --pretty

# Profile operator fixtures (depends on live reports)
python3 scripts/novaseal_profile_operator_fixtures.py --pretty

# Service builder fixtures (depends on operator fixtures)
python3 scripts/novaseal_service_builder_fixtures.py --pretty

# BTC SPV evidence adapter
python3 scripts/novaseal_btc_spv_evidence_adapter.py --pretty

# External attestation adapter
python3 scripts/novaseal_external_attestation_adapter.py --pretty

# External evidence handoff bundle (depends on both adapters)
python3 scripts/novaseal_external_evidence_handoff_bundle.py --pretty
```

### Phase 7: Certification

```bash
cargo run --locked -p cellscript --bin cellc -- \
  certify --plugin novaseal-profile-v0 --repo-root . --json
```

Expected output on success:
- `status: "passed"`
- `local_v1_ready: true`
- `production_ready: false` (external attestations not yet provided)
- `v1_status: "local_v1_ready_external_attestation_required"`

Full report: `target/novaseal-production-gates.json`

---

## 4. Expected Output Files

| File | Phase | Generated By |
|---|---|---|
| `target/novaseal-devnet-stateful-live.json` | 2 | Core live runner |
| `target/novaseal-agreement-devnet-stateful-live.json` | 3 | Agreement live runner |
| `target/novaseal-fungible-xudt-devnet-stateful-live.json` | 4 | Planned profile runner |
| `target/novaseal-rwa-receipt-devnet-stateful-live.json` | 4 | Planned profile runner |
| `target/novaseal-btc-transaction-commitment-devnet-stateful-live.json` | 4 | Planned profile runner |
| `target/novaseal-btc-utxo-seal-devnet-stateful-live.json` | 4 | Planned profile runner |
| `target/novaseal-dual-seal-devnet-stateful-live.json` | 4 | Planned profile runner |
| `target/novaseal-fiber-candidate-devnet-stateful-live.json` | 4 | Planned profile runner |
| `target/novaseal-fiber-node-experiments.json` | 5 | Fiber experiments runner |
| `target/novaseal-bip340-tcb-review.json` | 6 | TCB review script |
| `target/novaseal-wallet-signing-vectors.json` | 6 | Wallet vectors script |
| `target/novaseal-profile-operator-fixtures.json` | 6 | Operator fixtures script |
| `target/novaseal-service-builder-fixtures.json` | 6 | Service builder script |
| `target/novaseal-btc-spv-evidence-adapter.json` | 6 | BTC SPV adapter script |
| `target/novaseal-external-attestation-adapter.json` | 6 | External attestation script |
| `target/novaseal-external-evidence-handoff-bundle.json` | 6 | Handoff bundle script |
| `target/novaseal-devnet-stateful-acceptance.json` | 7 | Certification reducer |
| `target/novaseal-production-gates.json` | 7 | Certification reducer |
| `target/cellscript-certification/novaseal-profile-v0.json` | 7 | Certification reducer |

---

## 5. BTC SPV Boundary

Local BTC-facing profile devnet evidence (phases 4-5) proves that the CellScript
BTC integration compiles, deploys, and processes transitions on a CKB devnet.
It does **not** prove Bitcoin mainnet or testnet inclusion.

Public BTC SPV evidence is an **external production gate** that requires:
- A real external SPV service operating on public Bitcoin data
- Non-placeholder `btc_txid`, `btc_block_hash`, `spv_proof_hash`
- Minimum 6 confirmations
- Evidence provider with a real identity (not placeholder)

Template: `proposals/novaseal/v0-mvp-skeleton/proofs/public_btc_spv_evidence.template.json`

Adapter request: `target/novaseal-btc-spv-evidence-adapter.json`

---

## 6. Failure Modes and Rerun Policy

| Failure | Cause | Fix |
|---|---|---|
| Script exits with `CKB RPC did not become ready` | Port conflict or stale node | Kill old ckb processes; use `--run-dir` for isolation |
| `artifact_hashes_match: false` | Rebuilt ELF after generating report | Re-run the affected live devnet script |
| `source_hash_matches: false` | Changed source tracked by provenance | Re-run the affected live devnet script |
| Bruno suite timeout | Fiber node slow start | Increase `--timeout-seconds`; check port availability |
| `unknown service invoicesrpc.Invoices` | LND built without `invoicesrpc routerrpc` | Rebuild LND with those tags |
| `cellc certify` shows `failed` | Any upstream gate failed | Read `target/novaseal-production-gates.json` for specific failed gates |
| Provenance stale after git rebase | Source tree hash changed | Re-run phases 2-6, then phase 7 |

**Rerun policy:** You may re-run individual phases independently. Each phase
writes to its own output file. Phase 7 reads all of them. Do not re-run phase 7
until the upstream reports are fresh.

---

## 7. Cleanup

```bash
# Kill any leftover CKB/Fiber/LND processes
pkill -f 'ckb.*run' || true
pkill -f fnn || true
pkill -f lnd || true
pkill -f bitcoind || true

# Remove stale run directories (keeps JSON reports)
rm -rf target/novaseal-*-live-*/ target/novaseal-fiber-node-experiments/
```

---

## 8. Validation Commands

After phase 7, run:

```bash
cargo fmt --all
cargo check --locked -p cellscript --all-targets
cargo test --locked -p cellscript novaseal
cargo run --locked -p cellscript --bin cellc -- \
  certify --plugin novaseal-profile-v0 --repo-root . --json
cargo test --locked -p cellscript
cargo clippy --locked -p cellscript --all-targets -- -D warnings
git diff --check
```

All must pass before claiming local V1 readiness.

---

## 9. Production Readiness

Local V1 readiness does **not** mean production readiness. Production requires
four external attestations that this runbook cannot generate:

1. **Public/shared CellDep attestation** — real CKB mainnet/testnet deployment
2. **External BIP340 TCB review** — independent security review
3. **Public BTC SPV evidence** — external SPV service on real Bitcoin chain
4. **RWA legal/registry review** — external legal review with real jurisdiction

See `proposals/novaseal/v0-mvp-skeleton/proofs/*.template.json` for the expected
structure of each attestation.

---

*This runbook is the single source of truth for full devnet acceptance.
Profile-specific docs link back here for prerequisites, freshness rules, and
the overall command sequence.*
