# Optional financial package reproduction

On 2026-11-07 the maintainer cancelled the mandatory requirement in
[#55](https://github.com/CellScript-Labs/CellScript/issues/55) and
[#60](https://github.com/CellScript-Labs/CellScript/issues/60) for a named
developer outside the implementation team to supply clean-checkout reproduction,
a report and transaction evidence. External reproduction is optional and no
longer blocks programme acceptance. This guide remains available for voluntary
reproduction. Implementation-team evidence is available; no external
reproduction is claimed, and internal agent runs retain their actual authorship.
Independent security review and all other evidence requirements are unchanged.

## Source and tools

Use an immutable source snapshot containing this work, based on `fc94b122` on
`0.35`. Record its actual commit or patch identity. The
working-tree implementation is not part of the published alpha.2 release; a
version string alone cannot identify its compiler/checker/client changes.

Use the exact repository Rust toolchain and locked Cargo inputs. The sibling
`ckb-sdk-rust` is the clean `v5.1.0` checkout at
`1fbf3d4c9b35ef90bdb9e6621a8d26edde6325ce`. The CKB source and executable must
match [the tracked pin](../scripts/ckb_acceptance_pin.json). The example verifies
the source revision and executable version and records their identities.
Supply local paths to those inputs; the example does not publish or deploy to
a public network.

Build the native compiler with the existing command:

```sh
cargo build --locked -p cellscript --bin cellc
```

The [public SDK example](../examples/ckb-sdk-builder/README.md#disposable-financial-node-example)
owns its complete flags and prerequisites. Use fresh run directories. Its
known private keys and disposable node are for local reproduction only.

## Execute the public portfolio

First execute the reference profile, replacing the paths with the actual pinned
local inputs:

```sh
cargo run --locked -p cellscript-ckb-sdk-builder-example --example finance_node -- \
  --ckb-repo /absolute/path/to/pinned-ckb-source \
  --ckb-bin /absolute/path/to/pinned-ckb \
  --cellc /absolute/path/to/cellc \
  --lane complete --profile reference \
  --run-dir target/finance-node/outside-reference
```

Then instantiate the second profile with different complete asset identities,
issuer/owner keys, fee, nonce and reserves:

```sh
cargo run --locked -p cellscript-ckb-sdk-builder-example --example finance_node -- \
  --ckb-repo /absolute/path/to/pinned-ckb-source \
  --ckb-bin /absolute/path/to/pinned-ckb \
  --cellc /absolute/path/to/cellc \
  --lane complete --profile second --fee-bps 50 --pool-nonce 2 \
  --reserve-a 200000 --reserve-b 600000 \
  --run-dir target/finance-node/outside-second
```

Each complete profile creates real enforcing assets, pools, LP, orders and
escrow. It executes both swap directions, partial/all circulating LP redemption,
multiple order fills and cancellation, C1 direct order/pool settlement, C2
shared payments and C3 exact-input/exact-output routing. The C2 maximum uses
two orders/four credits/32 inputs and outputs. Pure routes require no order
dependency and the trader owns no intermediate Y input.

Review the actual reports, raw signed transactions, predecessor Cells, package
locks, compiled source snapshots, exact artifact bundles, generated SDKs,
CellDeps, journals and node logs. Never patch the generated verifier or replace
its witness/signature encoding to make an example pass. Record genuine API,
documentation or environment failures as part of the reproduction.

The current source also checks actual node rejections for C3 recipient
substitution and C2 aggregate underpayment, with exact verifier identities and
exit codes. Invalid transactions are dry-run and are not broadcast. Actual
stale candidates are classified separately from Script rejection. The unknown
submission exercise deliberately discards a successful acknowledgement and
observes the original transaction; it does not execute a second payment.

For the current frozen full-operation and recovery scopes, use the separate
`complete-c4 --whole-operation-limits` and `operations --operations-limits`
commands in the public example's README. They snapshot `cellc` before all
subprocesses. The first executes genuine third-party trailing layouts for
C1/C2/C3; the second includes actual SDK timeout, pool/order conflicts,
updated-state rebuilding, wallet cancellation and private-node reorganisation.
The implementation team's final runs passed 26 and ten respective budget
samples, recorded in [the resource record](CELLSCRIPT_FINANCE_RESOURCES.md).
A clean rebuild may have different executable hashes: record and review that
identity difference under an explicit reproduction configuration, preserving
the original limits and observations. Editing hashes or thresholds silently
does not establish acceptance.

## Independent economics and byte replay

Run the owning focused producers and the independent models:

```sh
cargo test --locked -p cellscript --test finance_math -- --nocapture
cargo test --locked -p cellscript --test finance_quotes -- --nocapture
cargo test --locked -p cellscript --test finance_protocol -- --nocapture
cargo test --locked -p cellscript --test finance_orders -- --nocapture
cargo test --locked -p cellscript --test finance_allocations -- --nocapture
cargo test --locked -p cellscript --test finance_allocation_model -- --ignored --nocapture
cargo test --locked -p cellscript --test finance_settlement_model -- --ignored --nocapture
cargo test --locked -p cellscript --test finance_archive_replay -- --ignored --nocapture
```

The BigInt models do not import the production quotation implementation.
Exact-output input is also found by integer search. Allocation modelling reads
the actual table/order/output bytes, checks cumulative obligations and sums
every credit to each shared output. Economic validity and authorisation remain
separate: a valid table does not authorise using the allocator's refund mode
to execute underpaid orders.

The C1 model independently combines the actual pool/reserve, order/escrow and
intent bytes. Its 21 selected cases include genuine missing/substituted peers,
a third enforcing asset, reserve mismatch, user limits, 32/32 equivalent
trailing layouts, and cardinality violations. It verifies that both layout
candidates use the same predecessor set and that unrelated capacity returns
to its owner. Signature and operation-witness validity remain separate from
the independently calculated economics.

[Byte replay](CELLSCRIPT_FINANCE_ARCHIVE_REPLAY.md) creates a fresh VM Context
from archived inputs/dependencies, without recompiling application source or
repairing transactions. Expected policy rejection, local stale bookkeeping and
node acceptance have separate records. [Resource acceptance](CELLSCRIPT_FINANCE_RESOURCES.md)
checks explicitly frozen local VM case ceilings; the public-node resource
policy has its own measured scope and report identity.

For an independent reference validation of the fixed node experiment policy,
add `--resource-limits examples/ckb-sdk-builder/finance-node-resource-limits.json`
and use another fresh run directory. Review its scope and history first:
the implementation-team reference passed, while three second-profile timing
ceilings were exceeded in a retained pilot. A business execution success must
not be reported as timing acceptance when that run has violations.

## Acceptance disposition

| Milestone | Implementation-team evidence | Remaining acceptance/disposition |
| --- | --- | --- |
| M0/profile | Ordinary packages, D01–D15 contracts and exact creation/identity admission boundary | Independent reviewer not assigned |
| M1/custody and mathematics | Real asset/custody/LP supply checks, hostile transactions, wide arithmetic and independent models (#52/#53) | Scoped review and gate evidence pending |
| M2/AMM and public SDK | Complete pool/LP lifecycle and two actual node profiles (#54/#55) | External second-pool reproduction optional under the 2026-11-07 decision |
| M3/orders and C1 | Real escrow, cumulative fills/cancellation and no-B-inventory settlement with user-limit/refund/recipient negatives (#56/#57) | Scope-specific review remains separate; external reproduction optional |
| M4/C2–C4 | Shared aggregate payments, both routing amount modes, maximum cardinalities, permitted unrelated-cell layouts and hostile cases | Scope-specific review remains separate; external reproduction optional |
| M5/operations | Recovery, actual stale submissions and 164 selected VM cases under original plus additive ceilings; historical node v2 measured-scope acceptance | Second-profile timing failures, unmeasured scopes and review remain explicit; an unmeasured stage is not a zero-cost stage |
| M6/closure | Public guide and exact local evidence | Assess remaining M0–M6 conditions without the cancelled external-reproduction requirement |

The maintainer requested no gates/CI during this implementation. No passing
gate receipt or release waiver is inferred from that instruction. Gate/review
requirements remain explicit in the issue acceptance map, subject to the
external-reproduction decision above. Optional external reproduction is not an
independent application security audit or public-network deployment evidence.

No external report or external developer's transaction evidence is required for
acceptance. Useful context for a voluntary report includes authorship,
source/compiler/checker/SDK/node identities, commands and parameters, model and
execution results, artifact identities and encountered difficulties. Any claimed
result should retain its actual evidence scope; optional reports do not replace
the locally required execution, resource, review or gate records.
