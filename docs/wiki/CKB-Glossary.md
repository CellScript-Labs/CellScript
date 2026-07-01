# CKB Glossary

This page explains the CKB words that appear throughout the CellScript wiki.
Keep it nearby while reading the tutorials. The goal is not to replace the CKB
documentation; the goal is to make CellScript examples easier to read.

## Cell

A Cell is CKB's basic piece of state. A transaction consumes input Cells and
creates output Cells. CellScript tries to keep that movement visible with effects
such as `consume` and `create`.

In CellScript, a `resource`, `shared`, or `receipt` is a typed view over
Cell-backed state.

## Input Cell

An input Cell is a Cell being spent by the current transaction. When an action
consumes a resource, or when a lock protects a spend, you should think about the
input Cells involved.

In lock syntax, `protected T` means a typed view of one selected input Cell
guarded by the current lock invocation.

## Output Cell

An output Cell is a new Cell created by a transaction. In CellScript, `create`
materializes typed output data and attaches it to a lock:

```cellscript
create token = Token {
    amount,
    symbol
} with_lock(owner)
```

The output is not just a return value. It is new chain state.

## Lock Script

A lock script decides whether a Cell may be spent. CellScript `lock` entries
compile into spend-boundary predicates.

Use `require` inside locks for checks that should fail the current script
validation when false.

## Type Script

A type script checks state transition rules for Cells. CellScript `action`
entries are closer to type-script style transition logic: they describe the
inputs, invariants, and outputs of a state change.

Use `assert` inside actions for business-state transition checks.

## TYPE_ID

TYPE_ID is the CKB convention for giving a Cell lineage a unique type identity
derived from the first input and output index chosen by the transaction
builder. In CellScript 0.15, `identity(ckb_type_id)` and
`create_unique<T>(identity = ckb_type_id)` surface that lifecycle explicitly in
source and metadata.

The compiler can require the TYPE_ID output plan and preserve TypeHash on
replacement, but the transaction builder still has to construct the concrete
TYPE_ID-compatible output.

## Identity Policy

An identity policy says how CellScript should recognize the same logical Cell
across lifecycle operations. Supported v0.15 policies are `ckb_type_id`,
`field(name)`, `script_args`, and `singleton_type`.

`replace_unique` emits runtime preservation checks for the selected policy.
`create_unique` emits a local runtime anchor for the created output and records
the full create-time uniqueness proof as runtime-required. Chain-wide
uniqueness remains a TYPE_ID builder-plan claim or a builder/indexer claim, not
a standalone compiler proof.

## Witness

Witness data is user-supplied transaction data. It can carry signatures,
parameters, or other bytes, but the data itself is not automatically authority.

In CellScript, `witness T` means typed data decoded from the transaction witness
surface. A `witness Address` is still just data unless a lock verifies a real
signature binding.

## Script Args

Script args are bytes stored in the executing script. They are often used to
bind a script to a particular owner, policy, or configuration.

CellScript uses `lock_args T` on lock parameters for typed fixed-width decoding
from the executing lock script's `Script.args`. This binds data to script args;
it does not by itself verify a transaction signature.

When `identity(script_args)` is used, ProofPlan reports the provenance as
`lock_args` rather than witness data. Runtime identity preservation is checked
through the CKB LockHash because script args are part of the lock script.

## Lock Group

CKB groups script execution by matching script. A lock may run over a script
group rather than an isolated Cell.

When CellScript says `protected T`, read it narrowly: one selected input Cell in
the current script group, not every Cell of type `T` in the transaction.

## Capacity

Capacity is CKB's storage resource. Output Cells must have enough capacity for
their data and scripts. Compiler metadata can describe capacity requirements,
but release evidence still needs builder-backed occupied-capacity checks.

## CellDep

A CellDep is a referenced Cell dependency. It lets a transaction use code or
read-only data without consuming that Cell.

CellScript records read-only accesses and deployment metadata so builders and
reviewers can see which dependencies must be present.

## DepGroup

A DepGroup packages multiple CellDeps behind one dependency reference. Release
metadata reports DepGroup policy so deployment and builder workflows can audit
which dependencies are being used.

## ProofPlan

ProofPlan is CellScript's audit metadata for verifier obligations. It records
where an obligation came from, which trigger and scope apply, what CKB views it
reads, which checks are covered by generated code, and which builder
assumptions remain.

Use `cellc explain proof` to read ProofPlan data in human-readable or JSON form.
If a plan says `runtime-required` or `gap:metadata-only`, it is not yet a fully
covered on-chain proof.

## Sighash

Sighash is the transaction digest scope used for signature verification. A
signature is only meaningful if you know what it signed.

CellScript does not hide sighash defaults. Future signature verification syntax
must expose digest mode, script group scope, witness layout, and replay
assumptions.
