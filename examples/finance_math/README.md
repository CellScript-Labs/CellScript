# Ordinary checked arithmetic library

Status: application-library implementation under
[#53](https://github.com/CellScript-Labs/CellScript/issues/53). These are ordinary
public CellScript functions, with no compiler or financial builtin extension.
The library does not authenticate assets, custody, pool identity, LP supply or
transaction settlement.

Add a path dependency named `finance_math`, then import functions from the
`finance_math` module. All operands and results use `u128`.

| Function | Contract |
| --- | --- |
| `mul_div_floor(a, b, d)` | Checked `u128` product, reject zero denominator, round down |
| `mul_div_ceil(a, b, d)` | Same product domain; round up using quotient/remainder, without `product + d - 1` |
| `isqrt(n)` | Exact nonnegative square-root floor for every `u128`; 64 bounded iterations |
| `verify_mul_div_floor(a, b, d, q, r)` | Check `d > 0`, `r < d` and exact `q*d + r = a*b` |
| `verify_mul_div_ceil(a, b, d, q, r)` | Check `d > 0`, `r < d` and exact `q*d = a*b + r` |
| `verify_isqrt(n, root)` | Check `root <= u64::MAX` and exact `root² <= n < (root+1)²` |

The direct product domain rejects overflow before division even when the
mathematical final quotient fits. The witness relations use existing C256
product comparisons and support full `u128` operands whose product exceeds
`u128`. A quotient outside `u128` has no valid representable witness. Callers
must enforce any narrower protocol amount, reserve, share and fee domains.
No full-width narrowing cast is needed.

The [recorded implementation comparison](../../docs/CELLSCRIPT_FINANCE_MATH_CHOICES.md)
compares individual direct/relation executions, their actual cycles, ELF and
witness bytes, the distinct product domains and unavailable measurements. The
bounded financial profile currently uses direct computation.

Ordinary functions use boolean domain predicates rather than verifier-boundary
`require` syntax. The caller must use `require verify_mul_div_floor(...)` (or
the corresponding ceil/root predicate). Invalid denominator, remainder or
root-width predicates return `false`; false C256 relations abort with the
existing arithmetic mismatch error. Direct functions evaluate the checked
product before division: product overflow takes priority over a zero
denominator, and an otherwise valid product with a zero denominator uses the
existing numeric failure. Quotient and remainder witnesses never replace
executing the relation checks.

The root algorithm visits bits 63 down to 0. Each candidate is at most
`2^64 - 1`, so its square fits `u128`. Choosing a bit exactly when that square
does not exceed `n` yields the maximal admissible root after 64 iterations.
The root-witness upper inequality is checked as `n+1 <= (root+1)²` inside C256;
both sides can equal `2^128` without overflowing a local `u128`.

`cargo test --locked -p cellscript --test finance_math -- --nocapture`
loads this package through the locked dependency resolver, validates each
scoped ELF through the independent artifact checker, and compares encoded
CKB-VM executions with one batched independent Node BigInt process. It retains
boundary, deterministic full-width and hostile witness cases and writes
`target/finance-math-evidence.json` with source/model identities, deployed ELF,
canonical witness/transaction sizes and observed successful-transaction cycles.
Failure cycles remain unavailable in that report. These measurements are
arithmetic comparisons, not frozen protocol budgets or financial operation
acceptance. Full custody, whole-transaction financial sequences and node
execution belong to the parent FIN programme. External reproduction became
optional under the maintainer's 2026-11-07 decision; it is not claimed here.
