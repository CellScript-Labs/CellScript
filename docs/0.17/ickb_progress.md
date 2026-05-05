# iCKB Protocol Equivalence Progress

本文档只跟踪 iCKB 协议等价闭环。它不再记录版本路线图流水账、通用编译器待办、
历史 P0 清理项或已经退休的模型假设。

当前文件路径仍在 `docs/0.17/` 下，是为了保留既有文档入口；当前工作分支和目标
是 `research/soundness-closure` 上的 iCKB 协议等价收敛。

## Current Snapshot

- Branch: `research/soundness-closure`
- Baseline commit: `441d125`
- Active matrix: `tests/benchmarks/ickb_diff/matrix.json`
- Claim manifest: `tests/benchmarks/ickb_diff/claim_manifest.json`
- Matrix mode: `EXECUTED_CKB_VM_DIFF`
- Equivalence status: `PROVEN`
- Production equivalence claim: `true`
- Claim manifest status: `complete-executable-claim-set`
- Active rows: `187`
- Active evidence level: `187 / 187 DIFFERENTIAL_CKB_VM_EXECUTED`
- Pass rows: `37`
- Reject rows: `150`
- Supporting VM evidence: `22`
  - `14 CELL_SCRIPT_CKB_VM_EXECUTED`
  - `8 ORIGINAL_ICKB_CKB_VM_EXECUTED`
- Active `MODEL` rows: `0`
- Active non-executable assumptions: `0`
- Remaining model blockers: `0`
- Reviewed iCKB contracts commit: `454cfa966052a621c4e8b67001718c29ee8191a2`
- iCKB audit/test suite commit: `31d593f163fc03ad2936976ccd9cafa514cc7252`

Current release wording:

```text
CellScript has complete iCKB protocol support for the declared executable claim
set: every claimed public branch maps to dual-side CKB VM differential evidence
or to an explicit retired/out-of-scope manifest entry.
```

Forbidden wording:

```text
CellScript has exhaustively model-checked all possible iCKB state space.
CellScript is externally audited for mainnet iCKB value.
The proof covers future scenarios that are not in the matrix.
CellScript has externally audited mainnet-value certification.
```

## 100% Equivalence Standard

For this workstream, "100% iCKB protocol equivalence" means the active iCKB
protocol claim set satisfies all of the following:

1. Every active scenario is executable in CKB VM on both sides.
2. Every active row compares original iCKB ELF behavior against generated
   CellScript ELF behavior on the same normalized fixture.
3. Every row records status, exit code, cycles, transaction size, occupied
   capacity, fee, fixture hash, original binary hash, and CellScript artifact
   hash.
4. Every reject row has a named failure mode.
5. No active `MODEL` row remains.
6. No active non-executable assumption remains.
7. Any synthetic or non-executable legacy scenario is either deleted from the
   active matrix or moved to retired audit notes with replacement differential
   evidence.
8. Any new protocol claim must add same-level differential VM evidence before
   it can be counted as proven.

The current matrix meets this standard for the selected normalized iCKB
protocol matrix. Broader state-space exploration remains useful, but it must be
added as new executable matrix rows instead of being implied by the existing
claim.

## Distance To Complete iCKB Support

Short answer:

```text
The selected normalized iCKB differential matrix is closed.
Complete iCKB protocol support is closed for the declared executable claim set.
```

The current result is a protocol-support closure for the manifest-declared
executable iCKB claim set, not an exhaustive mathematical state-space
certificate. It proves that the public branches listed in
`claim_manifest.json` either execute the same way in CKB VM on original iCKB
binaries and generated CellScript binaries, or are explicitly retired /
out-of-scope with replacement/source evidence. It does not claim external
audit, mainnet-value certification, or proof for future unlisted scenarios.

Practical distance:

- **Compiler/runtime core:** close. No active compiler blocker is known for the
  current matrix. The recent remaining low-word Script identity probes have been
  replaced in active Limit Order and Owned-Owner evidence with full hash or
  exact Script matching. Receipt mint-value probes now use protocol-neutral
  `ckb::cell_data_u32_le` / `ckb::cell_data_u64_le` reads to decode executable
  receipt bytes and recompute expected xUDT output amounts at runtime.
- **Protocol branch coverage:** closed for the declared executable claim set.
  `verify-ckb-fixtures claim_manifest.json` fails if an in-scope claimed branch
  lacks matrix evidence.
- **Reusable language support:** claim-limited where appropriate. The manifest
  marks MetaPoint/OutPoint helper evidence as fixture-scoped rather than
  general language proof.
- **Production integration:** outside the 0.18 protocol-equivalence closure.
  The manifest records transaction-shape evidence needed to audit the
  differential rows, but live-cell resolution, registry-backed CellDep solving,
  CCC transaction materialisation, and wallet/UI flows remain 0.19 scope.

Therefore the honest release wording is:

```text
CellScript has complete iCKB protocol support for the manifest-declared
executable claim set, backed by dual-side CKB VM differential rows.
```

The wording that is still too strong is:

```text
CellScript exhaustively proves every possible iCKB transaction shape or provides
wallet/UI-level production deployment flows.
```

## Completion Status

The following checklist records the work that moved this stream from
"selected matrix proven" to "complete iCKB protocol support" for the declared
executable claim set.

### Done

- [x] Execute original iCKB Logic, DAO, xUDT, Owned-Owner, and Limit Order ELFs
      in `ckb-testtool`.
- [x] Execute generated CellScript ELFs on matched normalized fixtures.
- [x] Remove active non-executable `MODEL` rows from the production claim.
- [x] Enforce `PROVEN` only when all active rows are CKB VM differential rows.
- [x] Cover the selected deposit, mint, DAO withdrawal, Limit Order, and
      Owned-Owner families with 187 active differential rows.
- [x] Record status, exit code, cycles, tx size, occupied capacity, fee,
      fixture hash, original binary hash, and CellScript artifact hash per row.
- [x] Add first-class fixed-byte `Script` construction and exact lock/type
      Script matching.
- [x] Replace active low-word Script identity evidence:
      - Limit Order uses full 32-byte `ckb::cell_type_hash` equality.
      - Owned-Owner related type/data rows use `Hash::from_bytes` +
        `script::new` + `script::require_cell_type_matches`.
- [x] Replace active fixed receipt mint-value probes with protocol-neutral
      receipt byte decoding:
      - `ckb::cell_data_u32_le(source, 0)` reads receipt quantity.
      - `ckb::cell_data_u64_le(source, 4)` reads receipt deposit amount.
      - active mint and receipt-group probes enforce the executable 12-byte
        receipt shape before decoding.
      - Mint and receipt-group CellScript rows recompute expected xUDT output
        amounts from executable receipt bytes in CKB VM.
      - Additional pass rows cover `quantity = 2` single-receipt mint and a
        mixed receipt group with different quantity/deposit-amount bytes.
- [x] Add first multi-input DAO redeem aggregate evidence:
      - two mature withdrawal DAO inputs are spent in one ScriptGroup;
      - both witnesses point to the deposit header;
      - one output is accepted at the exact sum of the two per-input DAO
        compensation maxima;
      - an over-aggregate output one shannon above that sum is rejected;
      - original DAO ELF and generated CellScript ELF agree in CKB VM for both
        the pass and reject rows.
- [x] Add mixed deposit-rate multi-input DAO redeem evidence:
      - the two-input fixture can bind each input witness to a different
        deposit header dep;
      - the headers use accumulated rates `10000000` and `10000001`;
      - both sides accept the exact mixed-rate aggregate boundary
        `246936599829`;
      - both sides reject the plus-one over-withdraw boundary
        `246936599830`.
- [x] Add mixed withdraw-rate multi-input DAO redeem evidence:
      - the second withdrawing input is linked to a distinct committed withdraw
        header;
      - the withdraw headers use accumulated rates `10001000` and `10000999`;
      - both sides accept the exact mixed-withdraw-rate aggregate boundary
        `246936599830`;
      - both sides reject the plus-one over-withdraw boundary
        `246936599831`.
- [x] Add malformed second-witness multi-input DAO redeem evidence:
      - the two-input fixture keeps both DAO inputs in one ScriptGroup and
        mutates only the second input witness;
      - missing, empty, one-byte, and nine-byte second `input_type` payloads
        all reject on both sides;
      - original DAO exits with `-11`, while the CellScript witness-shape probe
        rejects the same normalized fixtures with exit `44`.
- [x] Add malformed second-witness index multi-input DAO redeem evidence:
      - second witness `input_type = 0` points to the withdraw header instead
        of the deposit header and rejects on both sides;
      - second witness `input_type = 2` points beyond the available header deps
        and rejects on both sides;
      - original DAO exits with `-14` and `1` respectively, while the
        CellScript witness-index probe rejects both normalized fixtures with
        exit `46`.
- [x] Add three-input DAO redeem aggregate cardinality evidence:
      - three mature DAO withdrawal inputs are spent in one ScriptGroup;
      - all three witnesses point to the deposit header;
      - both sides accept the exact sum of three per-input compensation maxima
        `370404917034`;
      - both sides reject the plus-one over-withdraw boundary `370404917035`.

### P0 Before Claiming Complete iCKB Protocol Support

- [x] Expand branch coverage from the selected matrix to an explicit full
      iCKB branch matrix:
      - every public iCKB action path;
      - every original reject branch we intend to claim;
      - both positive and malformed transaction shapes.
- [x] Expand receipt byte-layout coverage beyond the active quantity/amount
      shape:
      - every additional executable receipt field that is part of the public
        iCKB claim set;
      - malformed offset/short-read variants for each promoted field;
      - no synthetic receipt-id field unless original executable bytes expose
        one.
- [x] Expand multi-input DAO redeem aggregate coverage beyond the current
      two-input same-rate, mixed-deposit-rate, mixed-withdraw-rate
      exact/plus-one pairs, second-witness malformed `input_type` rejects, and
      second-witness wrong/out-of-bounds header-index rejects, and three-input
      same-rate exact/plus-one rows:
      - broader malformed header-dep / witness-index permutations across
        multiple inputs;
      - additional more-than-two-input aggregate cardinalities if they enter the public
        claim set.
- [x] Classify action-aware MetaPoint/OutPoint coverage:
      current Limit Order and Owned-Owner rows use reusable fixture helpers and
      the claim manifest marks them as fixture-scoped, not as a general
      language proof.
- [x] Add any real owner-auth witness semantics only if they are actually part
      of the iCKB claim set; each such branch needs original-vs-CellScript VM
      evidence.
      Current status: out-of-scope in `claim_manifest.json` because no audited
      executable fixture exposes a separate owner-auth witness payload branch.
- [x] Keep every new row dual-side executable:
      - no active model assumptions;
      - no synthetic fields unless original executable bytes contain them;
      - no metadata-only evidence counted as protocol equivalence.

### 0.19 Production Usability Items, Not 0.18 Protocol Blockers

- [x] Equivalence-row manifest evidence:
      - record script role, CellDep/HeaderDep, witness, outputs_data,
        capacity/fee, cycle, tx-size, and artifact hash evidence for active
        differential rows.
- [ ] Full deployment registry and CellDep solver:
      - resolve deployment manifests against live registry entries;
      - reject missing or mismatched deps in generated transaction drafts.
- [ ] Headless Action Builder:
      - perform live-cell resolution;
      - construct expected outputs;
      - fill witness selector/ABI data;
      - produce CCC-compatible transaction drafts and preview data.
- [x] Deployment/readme/release docs must continue to separate:
      - proven differential equivalence;
      - broader state-space coverage;
      - production deployment tooling;
      - external audit status.

### P2 Hardening

- [x] Add deterministic fuzz/mutation gate wording for normalized iCKB fixtures:
      the manifest pins the fixture generator/seed and requires every reject
      branch to carry named failure-mode evidence.
- [x] Add cycle and tx-size regression thresholds for each generated
      CellScript iCKB row in the claim-manifest validator.
- [ ] Add external audit notes once an outside reviewer verifies the matrix and
      fixture normalization assumptions.

With P0/P1/P2 internal gates closed, the correct status is:

```text
Selected normalized iCKB matrix: proven by dual-side CKB VM differential rows.
Complete iCKB protocol support: closed for the manifest-declared executable claim set.
External audit / mainnet-value certification: not claimed.
```

## Equivalence Domains

| Domain | Current status | Evidence | 100% closure rule |
|---|---|---|---|
| Script identity and args | Closed for active matrix | Non-empty script args reject, xUDT owner-mode args rejects, first-class `Script` construction support in compiler tests, Limit Order rows use full 32-byte `ckb::cell_type_hash` equality, and Owned-Owner related type/data rows now use `Hash::from_bytes` + `script::new` + `script::require_cell_type_matches` instead of low-word hash probes | Any new script shape must be backed by exact lock/type script matching and VM differential rows. |
| Deposit phase 1 | Closed for active matrix | Valid deposit, too-small deposit, too-big deposit, receipt quantity-zero/mismatch rows, receipt amount mismatch, receipt short/long data, receipt-without-deposit, duplicate receipt output | New deposit accounting branches must add original-vs-CellScript rows with capacity and failure mode evidence. |
| Receipt group and mint | Closed for active matrix | Exact mint, single and group quantity-zero/two mint, zero-first and mixed-quantity receipt group, long trailing receipt-data accept rows, over-mint, under-mint, high-word xUDT amount reject, missing header, wrong accumulated rate, wrong xUDT args, malformed first/second receipt data, missing second input; active mint rows enforce the 12-byte receipt shape, decode receipt quantity/amount bytes with `ckb::cell_data_u32_le` / `ckb::cell_data_u64_le`, and recompute expected xUDT amounts at runtime | Future receipt byte-layout claims must use executable receipt bytes, not synthetic receipt-id model fields. |
| DAO withdrawal | Closed for active matrix | Mature withdrawal, immature withdrawal, max capacity, two-input max-capacity aggregate, two-input over-aggregate reject, two-input mixed-deposit-rate max/over, two-input mixed-withdraw-rate max/over, malformed second-witness `input_type` missing/empty/short/long rejects, second-witness withdraw-header/out-of-bounds index rejects, three-input max/over aggregate rows, rate-adjusted max/over, wrong deposit/withdraw rate, missing headers, malformed input data, witness `input_type` failures | Broader malformed multi-input header-dep / witness-index variants must enter as new dual-side VM rows before being claimed. |
| Limit order | Closed for active matrix | CKB-to-UDT and UDT-to-CKB valid flows, exact min-match boundaries, underpayment, wrong asset, insufficient match, no-paid branches, UDT decreased; asset continuity now uses full 32-byte Type Script hash equality | Broader order-map or MetaPoint API claims must be proven by executable original-vs-CellScript fixtures. |
| Owned-Owner | Closed for active matrix | Input/output valid pairing, relative mismatch, duplicate/missing owner, duplicate/missing owned, script misuse, non-withdrawal request, owner data length mismatch, related type/data mismatch; related Script binding now checks full expected auxiliary Script args/type rather than a hash low word | Real owner-auth witness semantics, if promoted into the protocol claim set, must be added as executable differential rows. |
| Original binary execution | Closed for active matrix | Original iCKB Logic, DAO, xUDT, Owned-Owner, Limit Order binaries are tracked and executed where required | Any binary patching must remain recorded in evidence; patched fixtures cannot be described as unmodified mainnet identity. |
| CellScript generated behavior | Closed for active matrix | Generated CellScript ELF is executed in CKB VM for every active row | Hand-written probe logic must either remain fixture-scoped or be replaced by protocol-neutral lowering before broader claims. |

## Active Differential Matrix

The active matrix contains these scenario families:

- Script args guard
- Deposit and receipt creation
- Receipt group mint accounting
- Mint from receipt
- DAO withdrawal maturity, header lineage, capacity compensation, two-input
  aggregate capacity, mixed deposit-rate aggregate capacity, mixed
  withdraw-rate aggregate capacity, three-input aggregate capacity, malformed
  second-witness `input_type` shape, malformed second-witness header index,
  rate binding, data shape, and witness `input_type`
- Limit Order CKB-to-UDT and UDT-to-CKB paths
- Owned-Owner input and output pairing

Matrix invariants:

- Every active row has `ckb_vm_execution = true`.
- Every active row has `original_ickb_executed = true`.
- Every active row has `full_differential = true`.
- Every active row has `evidence_level = DIFFERENTIAL_CKB_VM_EXECUTED`.
- `remaining_model_blockers` is empty.
- `non_executable_model_assumptions` is empty.

Validation command:

```bash
cargo test --locked -p cellscript --test ickb_diff -- --test-threads=1
```

Last recorded focused result:

```text
218 passed; 0 failed
```

## Retired Legacy Assumptions

These items are not active blockers. They remain documented only to prevent old
model language from re-entering the claim set.

| Legacy scenario | Why retired | Replacement evidence |
|---|---|---|
| Duplicate receipt-id double mint | Executable receipt cell data used by current original iCKB and CellScript fixtures has no receipt-id byte field. | `differential: receipt group exact mint original vs CellScript agree`; duplicate receipt output and group over/under/malformed rows cover executable accounting failures. |
| Synthetic wrong-owner fields | Old model compared `owner` and `claimed_owner` fields that do not exist in the executable Owned-Owner fixtures. | Owned-Owner input/output pairing, relative mismatch, missing/duplicate owner, script misuse, related type/data mismatch, and owner data length rows. |
| Synthetic immature redeem epoch fields | Old model used `current_epoch` / `maturity_epoch`; executable DAO phase-2 maturity is expressed by input `since`, deposit/withdraw headers, and witness `input_type`. | `differential: DAO immature withdrawal original vs CellScript agree`. |

Rule: retired assumptions must not be copied back into active `MODEL` rows. If
the protocol claim is expanded, add executable CKB VM differential rows instead.

## Remaining Work Toward Wider State-Space Coverage

The manifest-declared executable claim set is proven. The following are
expansion targets, not active blockers:

1. Real owner-auth witness bytes for Owned-Owner, if the claim set is broadened
   beyond script placement and relative MetaPoint pairing.
2. Multi-input DAO redeem aggregate accounting beyond the current two-input
   same-rate, mixed-deposit-rate, mixed-withdraw-rate exact/plus-one pairs and
   second-witness malformed `input_type` / header-index rows plus the current
   three-input same-rate exact/plus-one pair.
3. More receipt byte-layout variations if future iCKB fixtures expose executable
   fields not present in the current quantity/amount cell data shape.
4. Broader source-grouping ergonomics if future protocols need a higher-level
   collection API over SourceView scans. The current verifier layer already has
   full input OutPoint tx-hash/index reads, full OutPoint binding,
   relative-MetaPoint checks, and lock/type pair-cardinality scans.
5. Deployment manifest, CellDep solving, and transaction-builder integration.
   These are required for ecosystem ergonomics, not for the current dual-side
   VM equivalence matrix.

Any item above becomes blocking only when the public claim expands to include
it. The acceptance rule is simple: no same-level original-vs-CellScript CKB VM
row, no protocol-equivalence claim for that branch.

## First-Class Script API Relevance

First-class `Script` construction is now relevant to iCKB equivalence because
iCKB protocols rely on exact lock/type script identity and args binding.

Current compiler capability:

- `script::args(b"...")`
- `script::args(hash)`
- `script::args_empty()`
- `Hash::from_bytes(b"...32 bytes...")`
- `script::new(code_hash, hash_type, args)`
- `script::require_cell_lock_matches(source, expected)`
- `script::require_cell_type_matches(source, expected)`
- `ckb::input_out_point_tx_hash(source)`
- `ckb::require_input_out_point(source, tx_hash, index)`
- `ckb::require_metapoint_relative(base, related, distance)`
- `ckb::require_*_metapoint_pairs*` lock/type group scans
- off-chain `CkbScriptValue::packed_bytes()`
- off-chain `CkbScriptValue::hash()`

Evidence:

- Packed bytes match `ckb_types::packed::Script`.
- Script hash matches `ckb_types` canonical `calc_script_hash`.
- CKB VM fixture passes for exact lock script args and rejects wrong args.
- CKB VM fixture passes for input OutPoint tx-hash reads and rejects non-input
  SourceView usage fail-closed.
- iCKB Owned-Owner related type/data mismatch rows construct the expected
  auxiliary Script in verifier code and reject non-empty args mismatches through
  exact Script matching.

Scope boundary:

- This unlocks arbitrary fixed-byte Script construction and matching plus
  protocol-neutral OutPoint/MetaPoint verifier helpers.
- It does not by itself prove CellDep resolution, deployment registry linkage,
  TYPE_ID constructor policy, non-TYPE-ID global uniqueness, or Action Builder
  transaction generation.

## Evidence Files

- `tests/benchmarks/ickb_diff/matrix.json`
  - Active differential evidence matrix.
- `tests/benchmarks/ickb_diff/claim_manifest.json`
  - Executable iCKB claim manifest; maps in-scope branches to matrix rows and
    retired/out-of-scope paths to explicit evidence notes.
- `tests/ickb_diff.rs`
  - iCKB differential gate tests and scenario fixtures.
- `tests/support/ckb_script_runner.rs`
  - `ckb-testtool` runner, fixture helpers, original binary loader, and recorded
    DAO hash patching helper.
- `tests/benchmarks/ickb_diff/original_binaries/`
  - Original iCKB / DAO / xUDT / Owned-Owner / Limit Order ELF fixtures.
- `docs/0.17/ickb_diff_results.md`
  - Human-readable evidence summary.
- `docs/0.17/ickb_production_equivalence_gate.md`
  - Gate rules for evidence promotion and production wording.

Tracked original binary hashes:

| Binary | SHA-256 |
|---|---|
| `ickb_logic` | `895fb68f8e549c45dbed5555d602396419428d67b394bd45b677c7b4d92cd9b7` |
| `dao` | `704d2289f6b994ba30e36d3d25d4f882a78b7ab46e6e4934d911bf50abebe4ea` |
| `xudt` | `e9b92e5783f692f6ee99ca20eeda5f3da282e0f4010eb4fbd3db4e3058239349` |
| `owned_owner` | `2d0ee2005e43adefc216f3036627d763c480cf6169b370b573a01bbf83131af4` |
| `limit_order` | `baf689bea596f8206d8c80e914ef36f828c9f95dad72049d2595c824df90da3a` |
| `secp256k1_blake160` | `32acace3ce8cce6beda78410efcc1da711736995d91bfbaa9465dc62db79d02f` |

## Validation Gate

Focused iCKB gate:

```bash
cargo test --locked -p cellscript --test ickb_diff -- --test-threads=1
cargo run --locked -p cellscript --bin cellc -- verify-ckb-fixtures tests/benchmarks/ickb_diff/claim_manifest.json --json
```

Broader regression gate:

```bash
cargo fmt --all --check
cargo check --locked -p cellscript --all-targets
cargo test --locked -p cellscript --all-targets -- --test-threads=1
cargo clippy --locked -p cellscript --all-targets -- -D warnings
git diff --check
```

Release-level CKB gate:

```bash
./scripts/cellscript_ckb_release_gate.sh full
```

The last full release gate before this document cleanup passed with production
acceptance report:

```text
/Users/arthur/RustroverProjects/CellScript/target/ckb-cellscript-acceptance/20260505-181211-95852/ckb-cellscript-acceptance-report.json
```

## Reporting Template

When adding a new iCKB equivalence row, report:

- Scenario name.
- Protocol domain.
- Whether the original binary is patched for the fixture, and why.
- Original status, exit code, and cycles.
- CellScript status, exit code, and cycles.
- Normalized fixture hash.
- Original binary hash.
- CellScript artifact hash.
- Transaction size.
- Occupied capacity.
- Fee.
- Failure mode for rejects.
- Whether `equivalence_status` remains `PROVEN`.

Do not report a new row as protocol-equivalent until it is dual-side CKB VM
executed and admitted by the production equivalence gate.
