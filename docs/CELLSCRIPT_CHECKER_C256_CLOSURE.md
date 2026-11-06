# Checked C256 runtime closure and fixed-width borrows

This is the narrow general prerequisite tracked by
[#103](https://github.com/CellScript-Labs/CellScript/issues/103). It changes
independent artifact admission, not source syntax, compiler arithmetic, emitted
runtime code, witness encoding, error numbers or transaction semantics.

## Reproduced failure

[`document_product_sum_relation.cell`](../tests/fixtures/document_product_sum_relation.cell)
reads an ordinary typed `Document.revision`, then calls an ordinary private
function checking `q*d+r=a*b` through the existing C256 primitive. It has no
financial declarations or dependencies. Before the change, its generated ELF
failed with E2400/V2420 at the multiplication helper's output store:

```text
helper:exact_relation
  -> runtime:__c256_require_u128_sum2_products_eq
  -> runtime:__cellscript_mul_u128_to_u256
write without private stack provenance at 0x10ed8
```

The product-sum routine owns a 272-byte frame and passes an internal 32-byte
product buffer to the multiplication routine. The latter owns its separate
96-byte frame. Recursively checking multiplication as an ordinary helper lost
the incoming output pointer's caller ownership; the actual output was private
to its parent, rather than the multiplication routine itself.

The caller proof also exposed two existing transport contracts. `u128` uses a
16-byte pointer/exact-length ABI, but the earlier fixed-read parameter inventory
only covered hashes, byte arrays and structs. Following this borrowed input
through every real caller eventually requires proving its public witness
decoder, including pointer/length words after a7.

The compiler already emitted outgoing stack arguments, and baseline pool
programs with more than eight ABI words already compiled. This failure does
not establish that larger entries were universally unsupported. The newly
required fixed-input borrow proof selected the earlier fixed single-entry
decoder's register-only recognition boundary. The wider decoder below proves
that existing machine transport; it adds no new compiler ABI.

## Independently admitted machine effect

The checker validates the complete existing closure for these four resolved
runtime operations: unsigned product equality/ordering and checked sum of two
products equality/ordering. A symbol identifies which contract to prove; it
does not authorize trusting a body.

- The LE loader reads exactly eight individual bytes in the declared order.
- Multiplication uses all four 64-bit partial products, unsigned high halves
  and the exact carry chain. It writes exactly four limbs at offsets
  0/8/16/24 through the pointer supplied by its checked parent.
- Addition reads four limbs from each input, writes four output limbs and
  returns its final carry. Each product-sum side must reject a carry before
  the high-to-low comparison.
- The top-level requirements use their exact 112/272-byte frames, private
  product/sum buffers and saved return-address slots. Every operand guard,
  dependency call target, arithmetic instruction, branch, error 18/49 return
  and epilogue is checked. Contiguous block ownership and external entry into
  the middle of any dependency are also checked.

Only after that complete proof may a typed caller use a **read-only** effect
for its four/eight 16-byte operands. No write through an arbitrary incoming
pointer is permitted, and generic callee private-frame bounds are unchanged.
The complete closure is validated even in programs that need no nested-memory
caller summary.

An incoming `u128` borrow retains its actual ABI word, 16-byte extent and
offset. Every ordinary caller must transport the matching pointer/length;
the chain must reach a checked public decoder or independently owned bytes.
The public decoder copies/guards the actual WitnessArgs input_type payload.
A source type alone cannot establish an external pointer's provenance.

## Fixed single-entry stack transport

The existing fixed payload ceiling remains 1536 bytes. For a single entry,
the ABI word count derives the outgoing extent:
`align16(max(words-8,0)*8)`, bounded to 2047 bytes. Recognition checks:

- the complete existing witness guards and exact payload width;
- each register argument and each staged stack argument at its exact negative
  offset before adjusting SP, including a pointer in a7 whose length is word 8;
- the exact reservation/restoration around the call and its exact entry target;
- wrapper metadata frame `5376 + outgoing`, with the existing return-address
  save/restore and disjoint private witness storage.

The separate compact PolicyWitness adapter profile retains its eight-word
limit. No projection/report budget is increased, and no arbitrary dynamic,
Script.args or alternate entry transport is admitted by this change.

## Owning functions and evidence

| Owner | Responsibility |
| --- | --- |
| `nested_machine/product_buffers::{validate_all,validate,check_body,call}` | Complete runtime inventory, finite body, extent, branch and dependency-target proof |
| `product_buffers::{load,multiply,add,requirement}` | Independently written arithmetic/frame instruction contracts |
| `lifetime::Proof::entry` | Apply only the proved read-only top-level effect at an actual call |
| `lifetime/hash_reads::{parameters,hash_read,hash_arguments}` | Fixed u128 borrow seed, bounded extent and every-caller transport proof |
| `entry_codec::{parameters,parameters_with_stack}` | Keep compact policy parameters separate from bounded single-entry stack parameters |
| `entry_codec/single::{verify,fixed_stack_arg,fixed_immediate_arg}` | Exact existing register/stack decoder and wrapper frame |

[`tests/artifact_checker/product_buffers.rs`](../tests/artifact_checker/product_buffers.rs)
owns the nonfinancial regression. Standard and no-optimizations builds execute
all four operations with genuine transaction context, valid full-width inputs,
incorrect quotient/remainder and zero-denominator cases. Ordinary helper names
are changed without changing admission. Machine mutations cover the complete
closure's load/store/arithmetic/carry/condition families, caller operand
extents, split words 7/8, stack slots, SP reservations/restoration, frame metadata
and partial entry targets.

The four focused regression tests passed on the working implementation. Their
40 actual VM executions also include mathematically equal product sums that
overflow 256 bits: checked carry rejection remains error 49 instead of accepting
their wrapped values. More than 100 body mutation checks execute, alongside the
directed caller/decoder mutations. The pre-existing 1,106-case arithmetic
compatibility owner was rerun because global C256 body admission changed, and
passed; its deterministic report identity remained unchanged. These focused
results do not substitute for the matching gates.

Because the fixed decoder is shared, the existing `memory_hashes::` and
`cache_forwarders::` regression owners were also rerun: five and two tests
passed respectively. Their length/pointer/output-extent, complete callee,
private-storage, scalar-slot and frame mutations retain their previous
fail-closed behavior.

The owning checker package and affected integration-test targets also passed
the following focused lint commands with warnings denied:

```sh
cargo clippy --locked -p cellscript-artifact-checker --all-targets -- -D warnings
cargo clippy --locked -p cellscript --test artifact_checker --test finance_protocol -- -D warnings
```

The focused command is:

```sh
cargo test --locked -p cellscript --test artifact_checker product_buffers:: -- --test-threads=1
```

The separately frozen
[complete swap comparison](CELLSCRIPT_FINANCE_MATH_CHOICES.md) consumes the
ordinary-library relation through real asset, custody, pool, Type ID and
standard signature groups. Neither this regression nor that experiment proves
universal source equivalence, arbitrary runtime code, node performance or
application security. Dev/CI/backend gates and scope-specific review remain
separate requirements; no gate or CI was run for this work.
