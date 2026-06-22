# DOB-EVO/1 CellScript Swarm Audit

Branch: `nightly-0.20`
Path: `docs/0.20/CELLSCRIPT_0_20_DOB_EVO_SWARM_AUDIT.md`
Date: 2026-06-20
Method: Parallel cross-comparison of the DOB-EVO/1 evolving-DOB profile
(under `proposals/evolving-dob/evolving-dob-profile-v1/`) against its own
schemas, fixtures, proofs, scripts, and produced lock/registry/deployment
artefacts.

Three parallel audit passes ran:

1. **Semantic & schema pass** — read `src/evolving_dob_type.cell` end-to-end,
   cross-checked against the three schemas under `schemas/`.
2. **Coverage pass** — inventoried every `require` guard in the source and
   matched each one against the fixtures in `fixtures/` and the claims in
   `proofs/invariant_matrix.json` and `proofs/proofplan.json`.
3. **Tooling pass** — read both Python scripts in `scripts/`, the lockfile,
   `registry.json`, and `Deployed.toml`, and inspected the path/version/signal
   semantics.

This audit is read-only — no source, fixture, or tool was modified. All
findings have `file:line` references and reproducible reasoning.

---

## Executive Summary

DOB-EVO/1 is **structurally complete and correct as a state-transition
specification**: the three actions (`initialise_dob_state`,
`evolve_dob_state`, `finalise_dob_state`) are coherent, the schemas align
with the CellScript structs, the event-commitment hash tree is identical in
shape across the three actions, and the intent-echo guards preserve the
profile's invariants.

Original audit state before the 2026-06-22 closure pass:

1. The genesis action did not verify that an input cell locked by
   `intent.owner_lock` authorised the transaction. **Resolved** with
   `ckb::require_cell_lock_hash(source::input(0), intent.owner_lock)`.

2. **Adequately covered by negative fixtures.** Roughly 34 of 50 `require`
   guards have no dedicated negative fixture. Several safety-critical guards
   (action-byte discriminator, `previous_event_hash == zero` at genesis,
   `U64_MAX` overflow, `decoder_hash` mismatch, expiry equality across
   evolve/finalise) are uncovered.

3. Version-string drift, capability over-declaration, fabricated local
   `publisher_signature`, brittle `--keep-node`, and log-buffering rough edges
   are resolved. Remaining follow-ups are fixture depth, invariant matrix
   references, `PHASE_UNBORN` documentation, `released_at` regeneration,
   action-salt hardening, and minimum CKB version pinning for `data1`.

Post-audit correction: the original `REPO_ROOT` path finding was retracted
after re-checking `ROOT.parents[2]` on disk, and the pressure script's
`cargo run` fallback now explicitly selects `--bin cellc`.

---

## Findings Table

| # | Severity | Area | File:Line | Summary |
|---|---|---|---|---|
| 1 | HIGH | Security | `src/evolving_dob_type.cell:82-156` | **Resolved** — genesis now calls the checked CKB helper `ckb::require_cell_lock_hash(source::input(0), intent.owner_lock)`, avoiding the rejected raw fixed-byte equality obligation. |
| 2 | ~~HIGH~~ (see corrected note) | Tools | `scripts/evolving_dob_registry_pressure.py:18`, `scripts/evolving_dob_devnet_workflow.py:30` | `REPO_ROOT = ROOT.parents[2]` is **correct**. Original audit miscounted path levels. |
| 3 | MED | Coverage | `fixtures/*.json` | ~34 of 50 source guards have no negative fixture. |
| 4 | MED | Schema | `Cell.toml:13`, `scripts/evolving_dob_registry_pressure.py` | **Resolved** — `cellscript_version` is aligned with `compiler_version = "0.17.0"`, and the pressure gate now checks manifest/lock version equality. |
| 5 | MED | Security | `src/evolving_dob_type.cell:162-163,241-242` | **Resolved** — `evolve` and `finalise` now reject zero `old_state.spore_id` and `old_state.cluster_id` before preserving identity. |
| 6 | MED | Tools | `Deployed.toml`, `scripts/evolving_dob_devnet_workflow.py` | **Resolved for local devnet** — the fabricated `publisher_signature` was removed and local registry verification no longer requires it. Public registry promotion still requires a real signature. |
| 7 | LOW | Surface | `src/evolving_dob_type.cell:38` | **Resolved** — `burn` and `relock` capabilities were removed from `DobEvolutionStateV1`. |
| 8 | LOW | Coverage | `proofs/invariant_matrix.json` | Invariants lack `file:line` and `fixtures:` references; coverage is structurally invisible. |
| 9 | LOW | Schema | `Cell.lock:21` vs `Deployed.toml:2` | Deployment-record schema shape diverges (`v0.19` array vs lockfile `version = 1` flat). |
| 10 | LOW | Tools | `registry.json:15` | `released_at` is hardcoded — not regenerated on rebuild. |
| 11 | LOW | Robustness | `src/evolving_dob_type.cell:107,188,269` | Action salt strings use zero-padding to a fixed 32-byte length — fragile under future edits. |
| 12 | LOW | Docs | `src/evolving_dob_type.cell:10,77-80` | `PHASE_UNBORN` and `Unborn -> Active` flow edge describe a phase that never persists on a state cell. |
| 13 | LOW | Tools | `scripts/evolving_dob_registry_pressure.py:28` | **Resolved** — `cargo run` fallback now passes `--bin cellc`, matching the devnet workflow intent. |
| 14 | LOW | Tools | `scripts/evolving_dob_devnet_workflow.py` | **Resolved** — the CKB child starts in a new session, so `--keep-node` is not tied to the parent shell process group. |
| 15 | LOW | Tools | `scripts/evolving_dob_devnet_workflow.py` | **Resolved** — `ckb.log` is opened unbuffered in binary mode and the child no longer uses text-mode buffering. |
| 16 | LOW | Schema | `Deployed.toml:26` | `hash_type = "data1"` not pinned to a minimum CKB version. |

---

## 1. HIGH — Genesis did not verify input-cell authority — RESOLVED

**File**: `src/evolving_dob_type.cell:82-155`
**Action**: `initialise_dob_state`

In CKB, a TYPE_ID genesis transaction creates a brand-new state cell. The
type script runs *after* lock validation, so for the genesis action there is
no predecessor state cell whose lock can authorise the transaction.
`initialise_dob_state` asserts:

- `intent.spore_id != Hash::zero()` (line 88)
- `intent.cluster_id != Hash::zero()` (line 89)
- the new state cell's lock equals `intent.owner_lock` (line 133,
  `with_lock(intent.owner_lock)`)
- the receipt's lock equals `intent.owner_lock` (line 153)

It does **not** assert that any input cell in the transaction is locked by
`intent.owner_lock`. The standard CKB pattern for genesis is to require an
input cell whose `lock.hash() == owner_lock.hash()`. Without that check,
any user can submit an `initialise` transaction for a real `spore_id` they
do not own, pinning the evolving-DOB state line to an attacker-controlled
`owner_lock` forever.

**Resolution (2026-06-22)**: `initialise_dob_state` now calls:

```cellscript
ckb::require_cell_lock_hash(source::input(0), intent.owner_lock)
```

This uses the existing checked CKB SourceView helper
`__ckb_require_cell_lock_hash`, so production policy can classify the lock-hash
binding as an on-chain requirement helper instead of a raw
`fixed-byte-comparison` obligation. The fix is intentionally stricter and
simpler than a transaction-wide scan: input 0 must be authorised by the
declared owner lock.

---

## 2. ~~HIGH — Both scripts resolve `REPO_ROOT` to the wrong directory~~ — RETRACTED

**Files**:
- `scripts/evolving_dob_registry_pressure.py:18`
- `scripts/evolving_dob_devnet_workflow.py:30`

**Status: false positive.** The original audit's path-level trace was wrong.
Re-verified on disk:

```text
ROOT = /home/arthur/a19q3/CellScript_Private/proposals/evolving-dob/evolving-dob-profile-v1
ROOT.parents[0] = .../proposals/evolving-dob
ROOT.parents[1] = .../proposals
ROOT.parents[2] = /home/arthur/a19q3/CellScript_Private   ← this IS the repo root
ROOT.parents[3] = /home/arthur/a19q3
ROOT.parents[4] = /home/arthur
```

So `REPO_ROOT = ROOT.parents[2]` is the correct level. Confirmed by running
the devnet workflow with `CKB_BIN` set; the script locates the ckb binary via
`REPO_ROOT.parent / "ckb"` = `/home/arthur/a19q3/ckb` (which exists).

The original audit also claimed `proposals/evolving-dob/Cargo.toml` was used
as the manifest fallback. That is not what `parents[2]` resolves to — the
fallback path is the CellScript repo's `Cargo.toml`, which exists.

**No code change required.** The audit's other tooling findings (LOW #13,
#14, #15) remain valid.

---

## 3. MED — Negative fixture coverage is ~32 %

**Files**: `fixtures/*.json` and `proofs/invariant_matrix.json`

The source has **50 `require` guards** across the three actions. The
fixtures cover **~16** of them. Per-action breakdown:

### `initialise_dob_state` (20 guards; 4 covered)
| Line | Guard | Fixture |
|---|---|---|
| 86 | `version == 1` | `legacy_version_reject` |
| 88 | `spore_id != zero` | **MISSING** |
| 89 | `cluster_id != zero` | **MISSING** |
| 90-91 | old/new generation == 0 | **MISSING** |
| 92-93 | old_phase == Unborn, new_phase == Active | **MISSING** |
| 94 | old_dna_hash == zero | **MISSING** (only new_dna is tested) |
| 95 | new_dna_hash != zero | `dna_zero_reject` |
| 96-99 | traits/render symmetry | **MISSING** |
| 100 | rule_hash != zero | **MISSING** |
| 101 | decoder_hash != zero | **MISSING** |
| 102 | previous_event_hash == zero | **MISSING** |
| 103 | new_event_hash != zero | **MISSING** |
| 104 | expiry > now | **MISSING (init variant)** |
| 117 | new_event_hash == expected | `event_hash_mismatch_reject` (evolve only) |

### `evolve_dob_state` (15 guards; 8 covered)
| Line | Guard | Fixture |
|---|---|---|
| 161 | old_state.version == 1 | **MISSING** |
| 162 | old_state.phase == Active | `phase_regression_reject` (finalise variant only) |
| 163 | old_state.generation < U64_MAX | **MISSING** |
| 164 | old_state.expiry > now | `stale_state_reject` |
| 165 | intent.version == 1 | `legacy_version_reject` (init only) |
| 166 | intent.action == OP_EVOLVE | **MISSING** |
| 167-182 | echo checks | `stale_state_reject` (bundled) |
| 174 | new_dna_hash != zero | `dna_zero_reject` (init only) |
| 180 | decoder_hash echo | **MISSING** |
| 181 | owner_lock echo | `owner_lock_mismatch_reject` |
| 183 | new_event_hash != zero | **MISSING** |
| 184 | new_event_hash != latest_event_hash | **MISSING** |
| 185 | expiry == old_state.expiry | **MISSING** |
| 198 | new_event_hash == expected | `event_hash_mismatch_reject` |

### `finalise_dob_state` (15 guards; 4 covered)
| Line | Guard | Fixture |
|---|---|---|
| 242 | old_state.version == 1 | **MISSING** |
| 243 | old_state.phase == Active | `phase_regression_reject` |
| 244 | old_state.generation < U64_MAX | **MISSING** |
| 245 | old_state.expiry > now | **MISSING** |
| 246 | intent.version == 1 | `legacy_version_reject` (init only) |
| 247 | intent.action == OP_FINALISE | **MISSING** |
| 248-265 | echo checks | **MISSING (finalise variant)** |
| 253 | new_phase == Final | **MISSING** |
| 255 | new_dna_hash != zero | **MISSING (finalise variant)** |
| 261 | decoder_hash echo | **MISSING** |
| 262 | owner_lock echo | **MISSING (finalise variant)** |
| 266 | expiry == old_state.expiry | **MISSING** |
| 279 | new_event_hash == expected | **MISSING (finalise variant)** |

### Recommended new fixtures

Add at minimum:

- `action_byte_reject` per action (init/evolve/final): `intent.action`
  equals the wrong opcode.
- `init_previous_event_hash_nonzero_reject`: `intent.previous_event_hash`
  is not zero.
- `u64_overflow_reject`: `old_state.generation == U64_MAX` consumed by
  evolve.
- `decoder_hash_mismatch_reject` per action: `intent.decoder_hash !=
  old_state.decoder_hash`.
- `event_hash_replay_reject`: `intent.new_event_hash == old_state.latest_event_hash`.
- `expiry_drift_reject`: `intent.expiry != old_state.expiry`.
- `final_phase_intent_reject`: `intent.new_phase != PHASE_FINAL`.
- Per-action variants of `legacy_version_reject`, `dna_zero_reject`,
  `event_hash_mismatch_reject`, `owner_lock_mismatch_reject`,
  `stale_state_reject`.

---

## 4. MED — `cellscript_version` / `compiler_version` drift — RESOLVED

**Files**:
- `Cell.toml:13` → now `cellscript_version = "0.17.0"`
- `Cell.lock:13` → `compiler_version = "0.17.0"`
- `registry.json:10` → `cellscript_version = "0.17.0"`
- `Deployed.toml:10` → `compiler_version = "0.17.0"`

**Resolution (2026-06-22)**: the manifest now matches the existing lock,
registry, and deployment compiler version. The pressure gate parses
`Cell.toml` and `Cell.lock` and rejects future manifest/lock version drift.

---

## 5. MED — `cluster_id` / `spore_id` zero-checks missing on the state side — RESOLVED

**Files**: `src/evolving_dob_type.cell:157-185,238-266`

Only `initialise_dob_state` checks `intent.cluster_id != Hash::zero()`
(line 89) and `intent.spore_id != Hash::zero()` (line 88). The echo checks
in `evolve_dob_state` (lines 167-168) and `finalise_dob_state` (lines
248-249) preserve the value, but they do not reject a state line whose
*own* `cluster_id` or `spore_id` is zero. If a zero-id state cell ever
enters the live chain (via a fork, off-chain indexer mistake, or a future
action), `evolve` and `finalise` would happily continue evolving it.

**Resolution (2026-06-22)**: both successor actions now require non-zero
`old_state.spore_id` and `old_state.cluster_id` before any state transition is
accepted.

---

## 6. MED — `publisher_signature` is a content id, not a real signature — RESOLVED FOR LOCAL DEVNET

**Files**: `Deployed.toml:36` and `scripts/evolving_dob_devnet_workflow.py:270`

The previous local workflow wrote a `"local-devnet-workflow:" + tx_hash`
content identifier into `publisher_signature` and then required publisher
signature presence during local registry verification. That was misleading
because no cryptographic publisher signature was being verified.

**Resolution (2026-06-22)**: the local devnet workflow no longer writes a fake
`publisher_signature`, and local offline/live registry verification no longer
uses `--require-publisher-signature`. A real cryptographic publisher signature
remains a public-registry promotion requirement.

---

## 7. LOW — Declared but unused capabilities — RESOLVED

**File**: `src/evolving_dob_type.cell:38`

The original source declared `burn` and `relock`, but no action in v1 invoked
them. Over-broad capability declarations widen the attack surface that the type
system must defend.

**Resolution (2026-06-22)**: the resource now declares only
`store, create, consume, replace`.

---

## 8. LOW — `invariant_matrix.json` lacks file/line and fixture references

**File**: `proofs/invariant_matrix.json`

The matrix lists 10 invariants but the `location` strings are vague
("intent echo checks", "owner_lock fields", "EVOLVING_DOB_VERSION guards
in all actions") and no invariant lists a fixture or set of fixtures
that proves it. Coverage gaps like those in finding 3 are therefore
structurally invisible from the matrix alone.

**Recommended fix**: replace each `location` with `file:line` references
(e.g. `src/evolving_dob_type.cell:86,165,246`) and add a `fixtures` array
per invariant listing the `*_reject` fixture IDs that exercise it.

---

## 9. LOW — Deployment-record schema shape divergence

**Files**: `Cell.lock:21` (`[deployment.devnet]`) vs `Deployed.toml:2`
(`schema = "cellscript-deployed-v0.19"`)

The lockfile uses a flat `[deployment.devnet]` table; the deployment record
uses a `[[deployments]]` array-of-tables under `v0.19`. Consumers that
parse both will see two different deployment-record shapes.

**Recommended fix**: pick one schema (the v0.19 array-of-tables is newer)
and align `Cell.lock` to it, or document the divergence.

---

## 10. LOW — `released_at` is hardcoded

**File**: `registry.json:15`

```json
"released_at": "2026-06-16T17:13:40Z"
```

There is no script in the repo that regenerates `registry.json` from
`Cell.lock`. A rebuild will produce a fresh lockfile but the registry entry
will still claim an old release time.

**Recommended fix**: have the publish / dry-run pipeline write
`released_at = now` during registry regeneration, or regenerate the file
from `cellc publish` output.

---

## 11. LOW — Fragile 32-byte action-salt padding

**File**: `src/evolving_dob_type.cell:107,188,269`

The three salts are exactly 32 bytes, achieved by zero-padding a
`DOB-EVO/1:<action>:event-hash:` prefix:

- init: `DOB-EVO/1:init:event-hash:000000` (6 trailing zeros)
- evolve: `DOB-EVO/1:evolve:event-hash:0000` (4 trailing zeros)
- final: `DOB-EVO/1:final:event-hash:00000` (5 trailing zeros)

This works today but any future edit that adds or removes a zero silently
changes the domain separator. Two actions whose salts collide under
hash_pair will produce identical event-commitment roots for the same
inputs.

**Recommended fix**: replace the manual padding with a fixed
`Hash::from_bytes(b"DOB-EVO/1:<action>:event-hash:")` truncated or
padded to 32 bytes by a single canonical helper, so future edits cannot
introduce collisions.

---

## 12. LOW — `PHASE_UNBORN` describes a phase that never persists

**Files**: `src/evolving_dob_type.cell:10,77-80,92,141`

`PHASE_UNBORN` is referenced in the `flow` block and the initialise action
intent (lines 92, 141) but **no `DobEvolutionStateV1` cell is ever
created with `phase == PHASE_UNBORN`** — initialise writes
`phase: PHASE_ACTIVE` (line 124). The `Unborn -> Active` flow edge is
therefore documentation only; nothing in the type system rejects a future
code path that does set `phase == PHASE_UNBORN` on a state cell.

**Recommended fix**: either (a) remove `Unborn` from the `flow` block and
the `PHASE_UNBORN` constant since it is unreachable on persisted state, or
(b) add a resource-level `require phase != PHASE_UNBORN` so Unborn is
structurally unreachable on a state cell, or (c) explicitly document that
Unborn is a witness-only intent phase used at genesis but never persisted.

---

## 13. LOW — Inconsistent `cargo run` fallback between scripts — RESOLVED

**File**: `scripts/evolving_dob_registry_pressure.py:28` vs
`scripts/evolving_dob_devnet_workflow.py:64`

The pressure script previously omitted `--bin cellc`:

```python
return ["cargo", "run", "--locked", "-p", "cellscript",
        "--manifest-path", str(REPO_ROOT / "Cargo.toml"), "--"]
```

The devnet script had the explicit `--bin cellc`. If `cellscript` is a
workspace with multiple bins, the pressure script's `cargo run` will fail
with "ambiguous which bin to run".

**Resolution (2026-06-20)**: `evolving_dob_registry_pressure.py` now passes
`--bin cellc` in the fallback command. The exact argument order differs from
the devnet script, but both commands select the same package, manifest, and
binary.

---

## 14. LOW — `--keep-node` is brittle — RESOLVED

**File**: `scripts/evolving_dob_devnet_workflow.py:351,501`

`subprocess.Popen([...], stdout=log, stderr=log, text=True)` does not pass
`start_new_session=True`. When `--keep-node` is set, line 501 skips
`terminate()`, the script writes the report, returns, and the OS reparents
`ckb` to init — but if the parent shell sends SIGHUP to its process group
on exit (the default for many interactive shells, and any invocation
without `nohup`), `ckb` is killed anyway.

**Resolution (2026-06-22)**: the devnet workflow starts the CKB child with
`start_new_session=True`, so `--keep-node` no longer depends on the parent
shell process group surviving.

---

## 15. LOW — CKB log buffer not flushed on hang — RESOLVED

**File**: `scripts/evolving_dob_devnet_workflow.py:350-351`

The log file is opened in text mode and `Popen` is given `text=True`. CKB's
stdout is line-buffered in this configuration; a hung child with buffered
output leaves an empty `ckb.log`, defeating post-mortem diagnosis.

**Resolution (2026-06-22)**: the devnet workflow opens `ckb.log` in unbuffered
binary mode and does not enable text-mode buffering on the child process.

---

## 16. LOW — `hash_type = "data1"` not pinned to a minimum CKB version

**File**: `Deployed.toml:26`

`hash_type = "data1"` is valid for newer CKB. The workflow script does
not verify the CKB node version supports it. With an older devnet binary,
the live cell would be created with a different effective type-hash.

**Recommended fix**: pin the minimum CKB version in `docs/PRODUCTION_READINESS.md`
and assert the node version in the devnet workflow.

---

## Cross-Cutting Recommendations

1. **Genesis authority gap (finding 1) — RESOLVED.** The profile now uses the
   checked `ckb::require_cell_lock_hash` helper rather than raw fixed-byte
   equality.
2. **`REPO_ROOT` path (finding 2) — RETRACTED.** Original audit miscounted
   path levels. `parents[2]` is correct.
3. **Add the missing fixtures from finding 3.** The current negative
   coverage leaves several load-bearing guards (U64_MAX overflow,
   expiry-drift, action-byte discriminator) without any executable test
   that they reject.
4. **Version drift, over-broad capabilities, fake local publisher signature,
   brittle `--keep-node`, and CKB log buffering are RESOLVED.**
5. **Update the invariant matrix to reference file:line and fixture IDs
   (finding 8), decide on `PHASE_UNBORN` (finding 12), and pin the minimum
   CKB version for `data1` (finding 16).**
6. **Pressure-script fallback (finding 13) — RESOLVED.** The fallback now
   passes `--bin cellc`; keep it aligned with the devnet workflow if either
   command shape changes again.

---

## Sources Audited

```
proposals/evolving-dob/evolving-dob-profile-v1/
├── Cell.lock
├── Cell.toml
├── Deployed.toml
├── README.md
├── docs/
│   ├── PRODUCTION_READINESS.md
│   ├── PROFILE.md
│   ├── REGISTRY_PRESSURE.md
│   └── SECURITY.md
├── fixtures/
│   ├── dna_zero_reject.json
│   ├── event_hash_mismatch_reject.json
│   ├── evolve_valid.json
│   ├── finalise_valid.json
│   ├── init_valid.json
│   ├── legacy_version_reject.json
│   ├── owner_lock_mismatch_reject.json
│   ├── phase_regression_reject.json
│   ├── replay_generation_reject.json
│   ├── rule_hash_mismatch_reject.json
│   └── stale_state_reject.json
├── proofs/
│   ├── invariant_matrix.json
│   └── proofplan.json
├── registry.json
├── schemas/
│   ├── evolving_dob_event_v1.schema
│   ├── evolving_dob_intent_v1.schema
│   └── evolving_dob_state_v1.schema
├── scripts/
│   ├── evolving_dob_devnet_workflow.py
│   └── evolving_dob_registry_pressure.py
└── src/
    └── evolving_dob_type.cell
```

Initial audit pass was read-only. Subsequent closure notes in this file record
the resolved genesis-authority fix, the retracted `REPO_ROOT` finding, and
the resolved pressure-script and local-devnet workflow fixes.
