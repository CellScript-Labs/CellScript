# Financial arithmetic implementation comparison

This FIN-03 decision uses existing measurements from
`target/finance-math-evidence.json`; it does not rerun or replace them. The
report has schema `cellscript-finance-math-evidence-v1`, 82 independently
calculated model rows, 1,106 encoded VM executions and deterministic seed
`0x53586d75a6e7c4a4`. Its SHA-256 is
`13c5ebc4accecff1509c06a78e2b2e116902eb1efb171b01e3be5fcbb02385fb`.
The report retains the exact library/model/application hashes and each
`measurements[].verified_bundle_id`, artifact hash, execution identity and
serialized transaction hash.

## Selected domains and execution policy

The ordinary [`finance_math`](../examples/finance_math/README.md) package offers
two different contracts:

- Direct floor/ceiling functions multiply in checked `u128`, then divide. They
  reject a nonrepresentable product even when the final quotient would fit.
  Checked product overflow precedes denominator evaluation: runtime code 49
  takes priority over a zero divisor; an otherwise valid product divided by
  zero produces code 20. Ceiling uses quotient/remainder, avoiding `n+d-1`.
- Witness predicates verify exact relations using existing C256 product
  comparisons. For floor, `q*d+r=a*b`; for ceiling, `q*d=a*b+r`; both require
  `d>0` and `r<d`. Those domain predicates return false and the calling action
  rejects with code 5. An incorrect exact relation rejects with code 49. A
  mathematically out-of-`u128` quotient has no valid representable witness.

For full `u128` operands, each product is at most `(2^128-1)^2`; adding the
single `u128` remainder still gives less than `2^256`. The exact relations
therefore fit the selected product representation. Square-root verification
admits roots through `u64::MAX`; its C256 upper comparison permits both
`n+1` and `(root+1)^2` to equal `2^128`. Direct square root visits 64 candidate
bits whose squares fit `u128`.

The current financial profile selects direct computation. Its checked amount,
reserve and total-share fields are at most `10^12`; its largest scaled swap
numerator is at most `10^28`, and its deposit/withdrawal/initialisation products
are at most `10^24`. These all fit `u128`. This choice keeps the current
transaction ABI independent of builder-computed quotient/remainder witnesses.
It is a bounded-profile choice, not evidence that direct computation is
universally faster. The witness predicates remain available for applications
with larger mathematical products. No direct 256-bit division implementation
was added or measured, and no financial opcode or implicit narrowing was added.

## Observed comparisons

These are individual actual successful fixture executions, not averages over
positive and hostile batches. Floor/ceiling rows use model `quotient-12`:
`a=b=10^12,d=10000,q=10^20,r=0`. Root rows use `root-12`:
`n=u128::MAX,root=u64::MAX`. `standard` and `diagnostic:ff` are the report's
exact diagnostic names; the latter is produced by
`CompilerDiagnostics::without_optimizations()` in the owning test.

| Diagnostics | Entry | Deployed ELF bytes | Transaction cycles | Witness bytes | Transaction bytes |
| --- | --- | ---: | ---: | ---: | ---: |
| standard | direct_floor | 3,264 | 10,098 | 92 | 432 |
| standard | witnessed_floor | 4,576 | 8,188 | 124 | 464 |
| standard | direct_ceil | 4,952 | 15,349 | 92 | 432 |
| standard | witnessed_ceil | 4,576 | 8,188 | 124 | 464 |
| standard | direct_root | 4,408 | 57,102 | 60 | 400 |
| standard | witnessed_root | 5,096 | 8,363 | 76 | 416 |
| diagnostic:ff | direct_floor | 3,264 | 10,102 | 92 | 432 |
| diagnostic:ff | witnessed_floor | 4,584 | 8,196 | 124 | 464 |
| diagnostic:ff | direct_ceil | 4,960 | 15,357 | 92 | 432 |
| diagnostic:ff | witnessed_ceil | 4,584 | 8,196 | 124 | 464 |
| diagnostic:ff | direct_root | 4,432 | 57,693 | 60 | 400 |
| diagnostic:ff | witnessed_root | 5,104 | 8,371 | 76 | 416 |

The relation-based quotient fixture adds 32 witness/transaction bytes for its
two additional `u128` fields; root verification adds 16 bytes. The fixture
also supplies an expected result for its assertion, so these absolute witness
sizes are not predictions for a different production entry ABI. ELF sizes are
per selected entry, not an estimate of a linked pool executable or combined
deployment footprint.

The same report separately demonstrates rounding and width distinctions:

- `quotient-4` has `a=2,b=3,d=4`: floor is 1 with remainder 2; ceiling is 2
  with complementary remainder 2. Under `standard`, direct floor/ceil use
  9,973/15,256 cycles and each witness relation uses 8,188 cycles.
- `quotient-9` has `a=b=d=u128::MAX`, so the exact quotient fits `u128` while
  the product does not. Direct floor and ceiling reject with code 49. Both
  witness relations accept: 8,179 cycles under `standard`, 8,187 under
  `diagnostic:ff`, with the witness/ELF sizes of their corresponding rows.
  These are different admitted domains, not a correctness disagreement.
- Deterministic wrong-quotient, wrong-remainder, remainder-bound and root
  cases retain their expected rejection codes in `executions[]`; their cycle
  values remain unavailable rather than being replaced with zero.

These measurements show a cycle/ABI/code-size tradeoff worth retaining. The
isolated report does not measure a complete alternative financial operation,
off-chain quotient generation time, signing, node RPC, failure cycles, guest
stack or peak memory. No whole-operation speedup or universal budget follows
from that isolated comparison. Complete financial transactions continue to use
their separately frozen [operation resource evidence](CELLSCRIPT_FINANCE_RESOURCES.md).

## Matched complete swap experiment

[`finance_pool_math_comparison.rs`](../tests/support/finance_pool_math_comparison.rs)
additionally compares two fully signed, complete transactions for the same
economic A-for-B swap. Both start with reserves 100000/500000, total LP 223606,
fee 30/10000 and input 1234 A. The independent Node BigInt model gives 6076 B,
successor reserves 101234/493924 and unchanged LP supply. All real asset,
reserve, pool, built-in Type ID and upstream standard signature groups execute
against each respective final transaction.

The original package retains its nine-parameter entry and bounded direct
computation. A test-only package copy adds two `u128` quotient/remainder witness
parameters and replaces only the mode-1 floor calculation with the ordinary
`finance_math::verify_mul_div_floor` relation. Five exact source replacements
each assert a single match. The full original/variant sources, literal patches,
hashes and separate frozen package contexts accompany the experiment. The
production protocol is unchanged. This exposed the separately tracked general
[checker prerequisite #103](CELLSCRIPT_CHECKER_C256_CLOSURE.md).

| Complete swap | Transaction cycles | Pool ELF bytes | All application ELF bytes | Witness bytes | Transaction bytes |
| --- | ---: | ---: | ---: | ---: | ---: |
| Original direct | 3,036,224 | 141,128 | 193,888 | 321 | 2,595 |
| Ordinary witness relation | 3,022,468 | 143,912 | 196,672 | 353 | 2,627 |

For this one actual operation, the relation saved 13,756 cycles while adding
2,784 deployed application bytes and 32 witness/transaction bytes. Application
ELF totals sum the five distinct application executables; external system
Scripts are common dependencies and are not included in that column. There is
no claim of a universal performance winner or an accepted production ABI
change. Two correctly re-signed variants with a forged quotient or remainder
preserve the economic outputs and reject at the Pool Lock with error 49.
Both algorithms additionally reject a correctly signed input witness
`2^64+1234` with the unchanged valid outputs and quotient/remainder: error 5
proves the upper half is not silently narrowed to the admitted low value 1234.

The two runs have independent real genesis histories in fresh local VM
Contexts, identical assets/holder/parameters and the same chosen Type ID anchor.
They are alternative histories, rather than two simultaneous instances sharing
one Type ID on a chain. The different pool ELF and ABI necessarily change its
controller, LP configuration, reserve Locks, predecessor transaction and signed
transaction bytes. These identities are all retained; the experiment does not
pretend to execute byte-identical transactions under different code.

`target/finance-math-operation-comparison.json` binds the operation, sources,
patches, full Script identities, mathematical result and per-variant metrics.
Its two v2 transaction archives are `pool-math-direct` and `pool-math-relation`
under `target/finance-protocol-evidence/`. Fresh byte replay uses these archives
without rebuilding the application. This is VM operation evidence; off-chain
quotient generation/signing time, node RPC, failure cycles, observed stack and
peak memory are not measured by this owner.

The owner is [`tests/finance_math.rs`](../tests/finance_math.rs); its batched
[`Node BigInt model`](../tests/support/finance_math_model.mjs) does not import
the production helpers. A later change to the arithmetic domain, algorithm or
witness ABI requires its own comparison and affected operation evidence.
